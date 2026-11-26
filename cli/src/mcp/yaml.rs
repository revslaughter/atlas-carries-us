//! Edits atlas.yaml in place. A write tool changes one region, one tour step
//! or the project's description; this module rewrites only the lines of that
//! entry, so the comments and the layout of the rest of the file stay as a
//! person wrote them. Each edit parses its own result and compares it with
//! the value it meant to write; when the two differ, it tries a plainer form.

use serde_json::{Map, Value};

/// The widest line that the folded text blocks aim for.
const WIDTH: usize = 78;

/// A change to one entry of atlas.yaml.
pub enum Change {
    /// Replace the item at a position of a top-level list, or add one at
    /// the end when the position equals the list's length.
    Item {
        /// The top-level key: `regions` or `tour`.
        key: &'static str,
        /// The item's position.
        at: usize,
        /// The new item.
        value: Value,
        /// True to insert the item before `at`, false to replace it.
        insert: bool,
    },
    /// Set keys of the item at a position of a top-level list, and drop
    /// the keys whose value is null. The item's other lines stay.
    ItemKeys {
        /// The top-level key: `regions`.
        key: &'static str,
        /// The item's position.
        at: usize,
        /// The keys and their new values.
        values: Vec<(&'static str, Value)>,
    },
    /// Replace a whole top-level list. Each new item that equals an old
    /// item keeps that item's lines and the comments above them, so a
    /// reorder or a deletion keeps the comments of the untouched items. An
    /// empty list removes the key.
    List {
        /// The top-level key: `regions` or `tour`.
        key: &'static str,
        /// The new items, in order.
        items: Vec<Value>,
    },
    /// Replace the value of a top-level key, or remove the key when the
    /// value is null. acu writes the whole section again.
    Top {
        /// The top-level key, such as `layout`.
        key: &'static str,
        /// The new value, or null.
        value: Value,
    },
    /// Set keys of a top-level mapping, and drop the keys whose value is
    /// null.
    Keys {
        /// The top-level key: `project`.
        key: &'static str,
        /// The keys and their new values.
        values: Vec<(&'static str, Value)>,
    },
}

/// Parses YAML text into JSON values, as acu reads atlas.yaml.
pub fn parse(text: &str) -> Result<Value, String> {
    serde_yaml_ng::from_str(text).map_err(|e| format!("atlas.yaml does not parse: {e}"))
}

/// Applies a change to the parsed file, giving the value that the edited
/// text must parse to.
fn expected(original: &Value, change: &Change) -> Result<Value, String> {
    let mut v = original.clone();
    let obj = v
        .as_object_mut()
        .ok_or("atlas.yaml holds no mapping at the top")?;
    match change {
        Change::Item {
            key,
            at,
            value,
            insert,
        } => {
            let list = obj.entry(*key).or_insert_with(|| Value::Array(Vec::new()));
            if list.is_null() {
                *list = Value::Array(Vec::new());
            }
            let list = list
                .as_array_mut()
                .ok_or(format!("{key} in atlas.yaml holds no list"))?;
            if *insert || *at == list.len() {
                if *at > list.len() {
                    return Err(format!(
                        "{key} holds {} entries; pick a position up to {}",
                        list.len(),
                        list.len() + 1
                    ));
                }
                list.insert(*at, value.clone());
            } else if *at < list.len() {
                list[*at] = value.clone();
            } else {
                return Err(format!("{key} holds {} entries", list.len()));
            }
        }
        Change::ItemKeys { key, at, values } => {
            let m = obj
                .get_mut(*key)
                .and_then(Value::as_array_mut)
                .and_then(|l| l.get_mut(*at))
                .and_then(Value::as_object_mut)
                .ok_or(format!("{key} in atlas.yaml holds no entry {}", at + 1))?;
            set_keys(m, values);
        }
        Change::List { key, items } => {
            if items.is_empty() {
                obj.remove(*key);
            } else {
                obj.insert(key.to_string(), Value::Array(items.clone()));
            }
        }
        Change::Top { key, value } => {
            if value.is_null() {
                obj.remove(*key);
            } else {
                obj.insert(key.to_string(), value.clone());
            }
        }
        Change::Keys { key, values } => {
            let m = obj
                .entry(*key)
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or(format!("{key} in atlas.yaml holds no mapping"))?;
            set_keys(m, values);
        }
    }
    Ok(v)
}

/// Sets keys of a mapping, and drops the keys whose value is null.
fn set_keys(m: &mut Map<String, Value>, values: &[(&'static str, Value)]) {
    for (k, val) in values {
        if val.is_null() {
            m.remove(*k);
        } else {
            m.insert(k.to_string(), val.clone());
        }
    }
}

/// Edits the text and returns the new text and how acu edited it: "in
/// place" when only the entry's lines changed, "section" when acu wrote the
/// whole top-level section again.
pub fn edit(text: &str, change: &Change) -> Result<(String, &'static str), String> {
    let original = parse(text)?;
    let want = expected(&original, change)?;
    let lines: Vec<&str> = text.lines().collect();
    let ends_nl = text.ends_with('\n') || text.is_empty();
    // A file whose lines end in CR LF keeps that ending.
    let nl = if text.matches("\r\n").count() * 2 > text.matches('\n').count() {
        "\r\n"
    } else {
        "\n"
    };
    let mut tries: Vec<(Option<Vec<String>>, &'static str)> = Vec::new();
    for pretty in [true, false] {
        tries.push((splice(&lines, change, &original, &want, pretty), "in place"));
    }
    tries.push((rewrite_section(&lines, change, &want), "section"));
    for (attempt, how) in tries {
        let Some(new_lines) = attempt else { continue };
        let mut out = new_lines.join(nl);
        if ends_nl || !out.is_empty() {
            out.push_str(nl);
        }
        if parse(&out).ok().as_ref() == Some(&want) {
            return Ok((out, how));
        }
    }
    Err(
        "acu cannot edit atlas.yaml in place without losing its comments; edit the file by hand"
            .into(),
    )
}

/// One top-level key and the lines it spans, `start` included and `end`
/// left out. Blank lines and comments just before the next key belong to
/// that key.
struct Section {
    /// The key.
    key: String,
    /// The key's line.
    start: usize,
    /// The line after the section's last line.
    end: usize,
}

/// Reports whether a line holds nothing but space or a comment.
fn is_filler(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with('#')
}

/// The number of spaces that start a line.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// Lists the top-level keys of a block mapping.
fn sections(lines: &[&str]) -> Vec<Section> {
    let mut starts = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with([' ', '\t', '#', '-']) || l.trim().is_empty() || l.starts_with("...") {
            continue;
        }
        if let Some((k, _)) = l.split_once(':') {
            starts.push((i, k.trim().trim_matches(['"', '\'']).to_string()));
        }
    }
    let mut out = Vec::new();
    for (n, (i, k)) in starts.iter().enumerate() {
        let mut end = starts.get(n + 1).map_or(lines.len(), |s| s.0);
        while end > i + 1 && is_filler(lines[end - 1]) {
            end -= 1;
        }
        out.push(Section {
            key: k.clone(),
            start: *i,
            end,
        });
    }
    out
}

/// Reports whether a key's line holds only the key (and maybe a comment),
/// so its value follows as a block.
fn opens_block(line: &str) -> bool {
    match line.split_once(':') {
        Some((_, rest)) => {
            let r = rest.trim();
            r.is_empty() || r.starts_with('#')
        }
        None => false,
    }
}

/// Finds the items of a block sequence in a section: their indent and the
/// lines each spans. A section in flow style gives `None`.
fn seq_items(lines: &[&str], sec: &Section) -> Option<(usize, Vec<(usize, usize)>)> {
    if !opens_block(lines[sec.start]) {
        return None;
    }
    let first = (sec.start + 1..sec.end).find(|&i| !is_filler(lines[i]));
    let Some(first) = first else {
        return Some((2, Vec::new()));
    };
    let n = indent(lines[first]);
    let is_item =
        |l: &str| indent(l) == n && (l[n..].starts_with("- ") || l[n..].trim_end() == "-");
    if !is_item(lines[first]) {
        return None;
    }
    let mut starts = Vec::new();
    for (i, &l) in lines.iter().enumerate().take(sec.end).skip(first) {
        if is_filler(l) {
            continue;
        }
        if is_item(l) {
            starts.push(i);
        } else if indent(l) <= n {
            return None;
        }
    }
    let mut items = Vec::new();
    for (k, &st) in starts.iter().enumerate() {
        let mut end = starts.get(k + 1).copied().unwrap_or(sec.end);
        while end > st + 1 && is_filler(lines[end - 1]) {
            end -= 1;
        }
        items.push((st, end));
    }
    Some((n, items))
}

/// The keys of a block mapping: each key, its first line, and the line
/// after its last.
type Keys = Vec<(String, usize, usize)>;

/// Finds the keys of a block mapping in the lines from `from` to `to`, at
/// indent `n`. With `dash`, the first line opens a list item (`- key: …`),
/// as in a region of `regions`. Gives `None` for any other shape.
fn map_keys(lines: &[&str], from: usize, to: usize, n: usize, dash: bool) -> Option<Keys> {
    let key_of = |l: &str, at: usize| -> Option<String> {
        let rest = &l[at..];
        if rest.starts_with(['-', '#', '[', '{']) {
            return None;
        }
        let (k, _) = rest.split_once(':')?;
        Some(k.trim().trim_matches(['"', '\'']).to_string())
    };
    let mut starts = Vec::new();
    for (i, &l) in lines.iter().enumerate().take(to).skip(from) {
        if dash && i == from {
            if n < 2 || indent(l) != n - 2 || !l[n - 2..].starts_with("- ") {
                return None;
            }
            starts.push((i, key_of(l, n)?));
            continue;
        }
        if is_filler(l) {
            continue;
        }
        if indent(l) == n {
            starts.push((i, key_of(l, n)?));
        } else if indent(l) < n {
            return None;
        }
    }
    let mut keys = Vec::new();
    for (k, (st, key)) in starts.iter().enumerate() {
        let mut end = starts.get(k + 1).map_or(to, |s| s.0);
        while end > st + 1 && is_filler(lines[end - 1]) {
            end -= 1;
        }
        keys.push((key.clone(), *st, end));
    }
    Some(keys)
}

/// The position of a key in [`KEY_ORDER`]; other keys sort last.
fn rank(k: &str) -> usize {
    KEY_ORDER
        .iter()
        .position(|o| *o == k)
        .unwrap_or(KEY_ORDER.len())
}

/// Rewrites the lines of the given keys in a block mapping and keeps every
/// other line, comments included. A key that the mapping lacks goes in
/// after the nearest key that [`KEY_ORDER`] puts before it, so
/// `descriptionDrafted` follows `description`. A key whose value `m` lacks
/// goes away. Returns the new lines for `from` to `to`.
#[allow(clippy::too_many_arguments)]
fn splice_keys(
    lines: &[&str],
    from: usize,
    to: usize,
    n: usize,
    dash: bool,
    values: &[&str],
    m: &Map<String, Value>,
    pretty: bool,
) -> Option<Vec<String>> {
    let keys = map_keys(lines, from, to, n, dash)?;
    if keys.is_empty() {
        return None;
    }
    // Each edit replaces the lines from `st` to `end` (an empty range
    // inserts); edits never overlap.
    let mut edits: Vec<(usize, usize, usize, Vec<String>)> = Vec::new();
    for k in values {
        let new = m
            .get(*k)
            .map(|v| emit_key(k, v, n, pretty))
            .unwrap_or_default();
        match keys.iter().find(|x| x.0 == *k) {
            Some((_, st, end)) => {
                let mut new = new;
                if dash && *st == from {
                    // The first key shares its line with the item's dash.
                    let first = new.first_mut()?;
                    *first = format!("{}- {}", " ".repeat(n - 2), &first[n..]);
                }
                edits.push((*st, *end, usize::MAX, new));
            }
            None => {
                let after = keys
                    .iter()
                    .filter(|x| rank(&x.0) <= rank(k))
                    .max_by_key(|x| (rank(&x.0), x.1))
                    .unwrap_or(keys.last()?);
                edits.push((after.2, after.2, rank(k), new));
            }
        }
    }
    // Apply from the bottom up, so the line numbers above stay right; two
    // inserts at one place land in the order of KEY_ORDER.
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.2.cmp(&a.2)));
    let mut out: Vec<String> = lines[from..to].iter().map(|l| l.to_string()).collect();
    for (st, end, _, new) in edits {
        out.splice(st - from..end - from, new);
    }
    Some(out)
}

/// Writes the new lines of the one entry that changed and keeps every other
/// line. Gives `None` when the file's shape does not allow it.
fn splice(
    lines: &[&str],
    change: &Change,
    original: &Value,
    want: &Value,
    pretty: bool,
) -> Option<Vec<String>> {
    let secs = sections(lines);
    let own = |v: &[&str]| v.iter().map(|l| l.to_string()).collect::<Vec<_>>();
    match change {
        Change::Top { .. } => None,
        Change::List { key, items } => {
            let Some(sec) = secs.iter().find(|s| s.key == *key) else {
                if items.is_empty() {
                    return Some(own(lines));
                }
                let mut out = own(lines);
                while out.last().is_some_and(|l| l.trim().is_empty()) {
                    out.pop();
                }
                out.push(format!("{key}:"));
                for it in items {
                    out.extend(emit_item(it.as_object()?, 2, pretty));
                }
                return Some(out);
            };
            if items.is_empty() {
                let mut out = own(&lines[..sec.start]);
                out.extend(own(&lines[sec.end..]));
                return Some(out);
            }
            let (n, old) = seq_items(lines, sec)?;
            let old_values = original[*key].as_array()?;
            if old_values.len() != old.len() {
                return None;
            }
            // Each old item, with the comments and blank lines above it.
            let mut prev = sec.start + 1;
            let mut blocks: Vec<Option<(usize, usize, usize)>> = Vec::new();
            for &(st, end) in &old {
                blocks.push(Some((prev, st, end)));
                prev = end;
            }
            let tail = prev;
            // First, each new item that equals an old item takes that item's
            // lines. Then each changed item takes the comments above the old
            // item with its ID, else above the old item at its position.
            let mut used = vec![false; old.len()];
            let mut pick: Vec<Option<(usize, bool)>> = vec![None; items.len()];
            for (k, it) in items.iter().enumerate() {
                if let Some(i) = (0..old.len()).find(|&i| !used[i] && old_values[i] == *it) {
                    used[i] = true;
                    pick[k] = Some((i, true));
                }
            }
            for (k, it) in items.iter().enumerate() {
                if pick[k].is_some() {
                    continue;
                }
                let by_id = it.get("id").filter(|v| !v.is_null()).and_then(|id| {
                    (0..old.len()).find(|&i| !used[i] && old_values[i].get("id") == Some(id))
                });
                let at = by_id.or((k < old.len() && !used[k]).then_some(k));
                if let Some(i) = at {
                    used[i] = true;
                    pick[k] = Some((i, false));
                }
            }
            let mut out = own(&lines[..sec.start + 1]);
            for (k, it) in items.iter().enumerate() {
                match pick[k].and_then(|(i, same)| blocks[i].map(|b| (b, same))) {
                    Some(((lead, st, end), true)) => {
                        out.extend(own(&lines[lead..st]));
                        out.extend(own(&lines[st..end]));
                    }
                    Some(((lead, st, _), false)) => {
                        out.extend(own(&lines[lead..st]));
                        out.extend(emit_item(it.as_object()?, n, pretty));
                    }
                    None => out.extend(emit_item(it.as_object()?, n, pretty)),
                }
            }
            out.extend(own(&lines[tail..sec.end]));
            out.extend(own(&lines[sec.end..]));
            Some(out)
        }
        Change::Item {
            key, at, insert, ..
        } => {
            let item = want[*key].as_array()?.get(*at)?.as_object()?;
            let Some(sec) = secs.iter().find(|s| s.key == *key) else {
                // No such section yet: add it at the end.
                let mut out = own(lines);
                while out.last().is_some_and(|l| l.trim().is_empty()) {
                    out.pop();
                }
                out.push(format!("{key}:"));
                out.extend(emit_item(item, 2, pretty));
                return Some(out);
            };
            let (n, items) = seq_items(lines, sec)?;
            let new = emit_item(item, n, pretty);
            let mut out = own(&lines[..sec.start + 1]);
            if items.is_empty() {
                out.extend(new);
                out.extend(own(&lines[sec.start + 1..]));
                return Some(out);
            }
            // Each item keeps the comments and blank lines above it (its
            // lead); a new item goes in above the lead of the item it
            // precedes.
            let mut pending = Some(new);
            let mut prev = sec.start + 1;
            for (k, &(st, end)) in items.iter().enumerate() {
                let lead = own(&lines[prev..st]);
                if k == *at && *insert {
                    out.extend(pending.take()?);
                    out.extend(lead);
                    out.extend(own(&lines[st..end]));
                } else if k == *at {
                    out.extend(lead);
                    out.extend(pending.take()?);
                } else {
                    out.extend(lead);
                    out.extend(own(&lines[st..end]));
                }
                prev = end;
            }
            out.extend(own(&lines[prev..sec.end]));
            if let Some(p) = pending {
                out.extend(p);
            }
            out.extend(own(&lines[sec.end..]));
            Some(out)
        }
        Change::ItemKeys { key, at, values } => {
            let item = want[*key].as_array()?.get(*at)?.as_object()?;
            let sec = secs.iter().find(|s| s.key == *key)?;
            let (n, items) = seq_items(lines, sec)?;
            let &(st, end) = items.get(*at)?;
            let names: Vec<&str> = values.iter().map(|v| v.0).collect();
            let new = splice_keys(lines, st, end, n + 2, true, &names, item, pretty)?;
            let mut out = own(&lines[..st]);
            out.extend(new);
            out.extend(own(&lines[end..]));
            Some(out)
        }
        Change::Keys { key, values } => {
            let m = want[*key].as_object()?;
            let sec = secs.iter().find(|s| s.key == *key)?;
            if !opens_block(lines[sec.start]) {
                return None;
            }
            let first = (sec.start + 1..sec.end).find(|&i| !is_filler(lines[i]))?;
            let n = indent(lines[first]);
            if n == 0 {
                return None;
            }
            let names: Vec<&str> = values.iter().map(|v| v.0).collect();
            let new = splice_keys(lines, first, sec.end, n, false, &names, m, pretty)?;
            let mut out = own(&lines[..first]);
            out.extend(new);
            out.extend(own(&lines[sec.end..]));
            Some(out)
        }
    }
}

/// Writes the changed top-level section again from its value, and keeps
/// every other section's lines.
fn rewrite_section(lines: &[&str], change: &Change, want: &Value) -> Option<Vec<String>> {
    let key = match change {
        Change::Item { key, .. }
        | Change::ItemKeys { key, .. }
        | Change::Keys { key, .. }
        | Change::List { key, .. }
        | Change::Top { key, .. } => *key,
    };
    let secs = sections(lines);
    let mut new = vec![format!("{key}:")];
    match &want[key] {
        Value::Null => new.clear(),
        Value::Array(items) => {
            for it in items {
                new.extend(emit_item(it.as_object()?, 2, false));
            }
        }
        Value::Object(m) => {
            for (k, v) in ordered(m) {
                new.extend(emit_key(k, v, 2, false));
            }
        }
        _ => return None,
    }
    if new.is_empty() && secs.iter().all(|s| s.key != key) {
        return Some(lines.iter().map(|l| l.to_string()).collect());
    }
    let own = |v: &[&str]| v.iter().map(|l| l.to_string()).collect::<Vec<_>>();
    match secs.iter().find(|s| s.key == key) {
        Some(sec) => {
            let mut out = own(&lines[..sec.start]);
            out.extend(new);
            out.extend(own(&lines[sec.end..]));
            Some(out)
        }
        None => {
            let mut out = own(lines);
            out.extend(new);
            Some(out)
        }
    }
}

/// The order in which acu writes the keys of an entry; other keys follow in
/// the order of their names.
const KEY_ORDER: &[&str] = &[
    "view",
    "by",
    "at",
    "id",
    "name",
    "title",
    "color",
    "paths",
    "default",
    "open",
    "text",
    "repo",
    "links",
    "description",
    "descriptionDrafted",
    "readingTip",
    "autolinks",
    "drafted",
];

/// Lists a mapping's keys in the order of [`KEY_ORDER`].
fn ordered(m: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut v: Vec<(&String, &Value)> = m.iter().collect();
    v.sort_by_key(|(k, _)| {
        (
            KEY_ORDER
                .iter()
                .position(|o| o == k)
                .unwrap_or(KEY_ORDER.len()),
            k.to_string(),
        )
    });
    v
}

/// Writes one item of a block sequence at an indent.
fn emit_item(item: &Map<String, Value>, n: usize, pretty: bool) -> Vec<String> {
    let mut out = Vec::new();
    for (i, (k, v)) in ordered(item).into_iter().enumerate() {
        let mut lines = emit_key(k, v, n + 2, pretty);
        if i == 0 {
            if let Some(first) = lines.first_mut() {
                *first = format!("{}- {}", " ".repeat(n), &first[n + 2..]);
            }
        }
        out.append(&mut lines);
    }
    if out.is_empty() {
        out.push(format!("{}- {{}}", " ".repeat(n)));
    }
    out
}

/// Writes one key and its value at an indent. Long text becomes a folded
/// block, text with line breaks a literal block, and lists and mappings a
/// flow collection. With `pretty` off, every text is one quoted line.
fn emit_key(k: &str, v: &Value, n: usize, pretty: bool) -> Vec<String> {
    let pad = " ".repeat(n);
    let key = flow_key(k);
    let Value::String(t) = v else {
        return vec![format!("{pad}{key}: {}", flow(v))];
    };
    let inner = " ".repeat(n + 2);
    let safe =
        pretty && !t.is_empty() && t == t.trim() && t.chars().all(|c| c == '\n' || !c.is_control());
    if safe && t.contains('\n') && !t.lines().any(|l| l.ends_with(' ')) {
        let mut out = vec![format!("{pad}{key}: |-")];
        for l in t.split('\n') {
            out.push(if l.is_empty() {
                String::new()
            } else {
                format!("{inner}{l}")
            });
        }
        return out;
    }
    if safe && !t.contains('\n') && !t.contains("  ") && n + key.len() + t.len() + 4 > WIDTH {
        let mut out = vec![format!("{pad}{key}: >-")];
        let mut line = String::new();
        for w in t.split(' ') {
            if !line.is_empty() && inner.len() + line.len() + 1 + w.len() > WIDTH {
                out.push(format!("{inner}{line}"));
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(w);
        }
        out.push(format!("{inner}{line}"));
        return out;
    }
    vec![format!("{pad}{key}: {}", flow(v))]
}

/// Writes a key bare when YAML allows it, else quoted.
fn flow_key(k: &str) -> String {
    let bare = k.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if bare {
        k.to_string()
    } else {
        Value::String(k.to_string()).to_string()
    }
}

/// Writes a value on one line in flow style: `{ view: symbol, name: "x" }`.
fn flow(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let parts: Vec<String> = ordered(m)
                .into_iter()
                .map(|(k, v)| format!("{}: {}", flow_key(k), flow(v)))
                .collect();
            if parts.is_empty() {
                "{}".into()
            } else {
                format!("{{ {} }}", parts.join(", "))
            }
        }
        Value::Array(a) => format!("[{}]", a.iter().map(flow).collect::<Vec<_>>().join(", ")),
        Value::String(s) if is_word(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Reports whether a string can stand bare in YAML and still read as the
/// same string: a short word such as `symbol` or `overview`.
fn is_word(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 40
        && s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        && !matches!(
            s,
            "yes" | "no" | "on" | "off" | "true" | "false" | "null" | "y" | "n"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEXT: &str = "# Top comment.\nversion: 1\nproject:\n  name: Shop\n  # About the shop.\n  description: |\n    Old text.\nlanguages:\n  - adapter: go\n\n# The tour.\ntour:\n  - title: One\n    open: { view: overview }\n    text: First.\n  # Before two.\n  - title: Two\n    open: { view: overview }\n    text: Second.\n";

    #[test]
    fn a_reordered_list_keeps_the_comments_of_each_item() {
        let v = parse(TEXT).unwrap();
        let mut steps = v["tour"].as_array().unwrap().clone();
        steps.reverse();
        steps.push(json!({"title": "Three", "open": {"view": "overview"}, "text": "Third."}));
        let (out, how) = edit(
            TEXT,
            &Change::List {
                key: "tour",
                items: steps.clone(),
            },
        )
        .unwrap();
        assert_eq!(how, "in place");
        assert_eq!(parse(&out).unwrap()["tour"], json!(steps));
        assert!(
            out.contains("# The tour.\ntour:\n  # Before two.\n  - title: Two\n"),
            "{out}"
        );
        assert!(out.contains("  - title: One\n"), "{out}");
        assert!(out.ends_with("    text: \"Third.\"\n"), "{out}");
    }

    #[test]
    fn a_changed_item_keeps_the_comment_above_it() {
        let v = parse(TEXT).unwrap();
        let mut steps = v["tour"].as_array().unwrap().clone();
        steps[1]["text"] = json!("Changed.");
        let (out, _) = edit(
            TEXT,
            &Change::List {
                key: "tour",
                items: steps.clone(),
            },
        )
        .unwrap();
        assert_eq!(parse(&out).unwrap()["tour"], json!(steps));
        assert!(
            out.contains("  # Before two.\n  - title: \"Two\"\n"),
            "{out}"
        );
        // A region keeps its comment by ID, even after a move.
        let text = "version: 1\nregions:\n  # About a.\n  - id: a\n    name: A\n  # About b.\n  - id: b\n    name: B\n";
        let items = vec![
            json!({"id": "b", "name": "B2"}),
            json!({"id": "a", "name": "A"}),
        ];
        let (out, _) = edit(
            text,
            &Change::List {
                key: "regions",
                items: items.clone(),
            },
        )
        .unwrap();
        assert_eq!(parse(&out).unwrap()["regions"], json!(items));
        assert!(out.contains("  # About b.\n  - id: b\n"), "{out}");
        assert!(out.contains("  # About a.\n  - id: a\n"), "{out}");
    }

    #[test]
    fn a_list_that_the_file_lacks_goes_in_at_the_end() {
        let items = vec![json!({"id": "a", "name": "A", "paths": ["x"]})];
        let (out, how) = edit(
            TEXT,
            &Change::List {
                key: "regions",
                items: items.clone(),
            },
        )
        .unwrap();
        assert_eq!(how, "in place");
        assert_eq!(parse(&out).unwrap()["regions"], json!(items));
        assert!(out.starts_with("# Top comment.\n"), "{out}");
    }

    #[test]
    fn an_empty_list_removes_its_section() {
        let (out, _) = edit(
            TEXT,
            &Change::List {
                key: "tour",
                items: vec![],
            },
        )
        .unwrap();
        assert!(parse(&out).unwrap().get("tour").is_none(), "{out}");
        assert!(out.starts_with("# Top comment.\n"), "{out}");
        // An empty list where none stood changes nothing.
        let (same, _) = edit(
            &out,
            &Change::List {
                key: "regions",
                items: vec![],
            },
        )
        .unwrap();
        assert_eq!(same, out);
    }

    #[test]
    fn a_top_key_is_set_and_removed() {
        let rows = json!({"rows": [["a", "b"], ["c"]]});
        let (out, how) = edit(
            TEXT,
            &Change::Top {
                key: "layout",
                value: rows.clone(),
            },
        )
        .unwrap();
        assert_eq!(how, "section");
        assert_eq!(parse(&out).unwrap()["layout"], rows);
        let (out, _) = edit(
            &out,
            &Change::Top {
                key: "layout",
                value: Value::Null,
            },
        )
        .unwrap();
        assert!(parse(&out).unwrap().get("layout").is_none(), "{out}");
        assert!(out.contains("# Before two."), "{out}");
    }

    #[test]
    fn replacing_a_step_keeps_every_other_line() {
        let step = json!({"title": "Two again", "open": {"view": "symbol", "name": "Mux.Get"}, "text": "New.", "drafted": {"by": "m", "at": "t"}});
        let (out, how) = edit(
            TEXT,
            &Change::Item {
                key: "tour",
                at: 1,
                value: step,
                insert: false,
            },
        )
        .unwrap();
        assert_eq!(how, "in place");
        assert!(
            out.contains("# Top comment.")
                && out.contains("  # Before two.")
                && out.contains("# The tour."),
            "{out}"
        );
        assert!(
            out.contains(
                "  - title: \"Two again\"\n    open: { view: symbol, name: \"Mux.Get\" }\n"
            ),
            "{out}"
        );
        assert!(out.contains("drafted: { by: m, at: t }"), "{out}");
        assert!(!out.contains("Second."), "{out}");
    }

    #[test]
    fn inserting_and_appending_steps_works() {
        let step = json!({"title": "Zero", "open": {"view": "overview"}, "text": "Before all."});
        let (out, _) = edit(
            TEXT,
            &Change::Item {
                key: "tour",
                at: 0,
                value: step.clone(),
                insert: true,
            },
        )
        .unwrap();
        assert_eq!(parse(&out).unwrap()["tour"][0]["title"], "Zero");
        assert_eq!(parse(&out).unwrap()["tour"][1]["title"], "One");
        let (out, _) = edit(
            TEXT,
            &Change::Item {
                key: "tour",
                at: 2,
                value: step,
                insert: true,
            },
        )
        .unwrap();
        assert_eq!(parse(&out).unwrap()["tour"][2]["title"], "Zero");
        assert!(out.ends_with("    text: \"Before all.\"\n"), "{out}");
    }

    #[test]
    fn a_missing_section_is_added_at_the_end() {
        let r = json!({"id": "core", "name": "Core", "paths": ["a", "a/**"], "description": "x ".repeat(60).trim()});
        let (out, how) = edit(
            TEXT,
            &Change::Item {
                key: "regions",
                at: 0,
                value: r.clone(),
                insert: true,
            },
        )
        .unwrap();
        assert_eq!(how, "in place");
        assert_eq!(parse(&out).unwrap()["regions"][0], r);
        assert!(out.contains("    description: >-\n"), "{out}");
    }

    #[test]
    fn setting_the_description_keeps_the_project_comments() {
        let (out, how) = edit(
            TEXT,
            &Change::Keys {
                key: "project",
                values: vec![
                    ("description", json!("Para one.\n\nPara two.")),
                    (
                        "descriptionDrafted",
                        json!({"by": "m", "at": "2026-01-01T00:00:00Z"}),
                    ),
                ],
            },
        )
        .unwrap();
        assert_eq!(how, "in place");
        assert!(
            out.contains(
                "  # About the shop.\n  description: |-\n    Para one.\n\n    Para two.\n"
            ),
            "{out}"
        );
        let v = parse(&out).unwrap();
        assert_eq!(v["project"]["descriptionDrafted"]["by"], "m");
        assert_eq!(v["languages"][0]["adapter"], "go");
    }

    #[test]
    fn a_flow_section_is_written_again() {
        let text = "version: 1\nproject: {name: x}\nlanguages: [{adapter: go}]\ntour: [{title: a, text: b, open: {view: overview}}]\n";
        let step = json!({"title": "c", "text": "d", "open": {"view": "overview"}});
        let (out, how) = edit(
            text,
            &Change::Item {
                key: "tour",
                at: 1,
                value: step,
                insert: true,
            },
        )
        .unwrap();
        assert_eq!(how, "section");
        assert_eq!(parse(&out).unwrap()["tour"][1]["title"], "c");
        assert!(out.starts_with("version: 1\nproject: {name: x}\n"), "{out}");
    }

    #[test]
    fn odd_text_round_trips() {
        for t in [
            "yes",
            "a: b # c",
            "- x",
            "  lead",
            "tab\there",
            "quote \" and \\ back",
            "ünï €",
        ] {
            let step = json!({"title": t, "text": format!("{t} {}", "w ".repeat(50)), "open": {"view": "overview"}});
            let (out, _) = edit(
                TEXT,
                &Change::Item {
                    key: "tour",
                    at: 0,
                    value: step.clone(),
                    insert: false,
                },
            )
            .unwrap();
            assert_eq!(parse(&out).unwrap()["tour"][0], step, "{out}");
        }
    }

    #[test]
    fn a_file_with_cr_lf_endings_keeps_them() {
        let text = TEXT.replace('\n', "\r\n");
        let step = json!({"title": "New", "open": {"view": "overview"}, "text": "x"});
        let (out, how) = edit(
            &text,
            &Change::Item {
                key: "tour",
                at: 2,
                value: step,
                insert: true,
            },
        )
        .unwrap();
        assert_eq!(how, "in place");
        assert_eq!(
            out.matches('\n').count(),
            out.matches("\r\n").count(),
            "{out:?}"
        );
        assert!(out.contains("# Top comment.\r\n"), "{out:?}");
    }

    #[test]
    fn changing_keys_of_an_item_keeps_its_other_lines() {
        let text = "version: 1\nproject: {name: x}\nlanguages: [{adapter: go}]\nregions:\n  - id: core # the heart\n    # Its name.\n    name: Router\n    paths: [a]\n    description: Old.\n  - id: rest\n    name: Rest\n";
        let (out, how) = edit(
            text,
            &Change::ItemKeys {
                key: "regions",
                at: 0,
                values: vec![
                    ("description", json!("New.")),
                    ("readingTip", json!("Start here.")),
                    ("drafted", json!({"by": "m", "at": "t"})),
                ],
            },
        )
        .unwrap();
        assert_eq!(how, "in place");
        assert!(
            out.contains("  - id: core # the heart\n    # Its name.\n    name: Router\n    paths: [a]\n    description: \"New.\"\n    readingTip: \"Start here.\"\n    drafted: { by: m, at: t }\n  - id: rest\n"),
            "{out}"
        );
        let (out, _) = edit(
            text,
            &Change::ItemKeys {
                key: "regions",
                at: 0,
                values: vec![("id", json!("heart"))],
            },
        )
        .unwrap();
        assert!(out.contains("  - id: heart\n    # Its name.\n"), "{out}");
    }

    #[test]
    fn a_new_key_follows_the_key_it_belongs_after() {
        let text = "version: 1\nproject:\n  name: x\n  description: Old.\n  repo: o/r\nlanguages: [{adapter: go}]\n";
        let (out, _) = edit(
            text,
            &Change::Keys {
                key: "project",
                values: vec![
                    ("description", json!("New.")),
                    ("descriptionDrafted", json!({"by": "m", "at": "t"})),
                ],
            },
        )
        .unwrap();
        assert!(
            out.contains(
                "  description: \"New.\"\n  descriptionDrafted: { by: m, at: t }\n  repo: o/r\n"
            ),
            "{out}"
        );
    }
}
