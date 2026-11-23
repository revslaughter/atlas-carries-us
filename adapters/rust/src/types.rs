//! Writes rustdoc's types, bounds and generics as Rust text, and collects the
//! IDs of the items they name, so that the map can link `Vec<Book>` to `Book`
//! and draw an edge from a function to the types it takes.

use rustdoc_types::{
    Abi, AssocItemConstraint, AssocItemConstraintKind, FunctionHeader, FunctionSignature,
    GenericArg, GenericArgs, GenericBound, GenericParamDef, GenericParamDefKind, Generics, Id,
    Path, Term, TraitBoundModifier, Type, WherePredicate,
};

/// Writes types and collects the rustdoc IDs that they name.
#[derive(Default)]
pub struct Printer {
    /// The rustdoc IDs that the written text names, in order.
    pub ids: Vec<Id>,
}

impl Printer {
    /// Writes a type and returns its text and the IDs it names.
    pub fn one(t: &Type) -> (String, Vec<Id>) {
        let mut p = Printer::default();
        let s = p.ty(t);
        (s, p.ids)
    }

    /// Writes a type.
    pub fn ty(&mut self, t: &Type) -> String {
        match t {
            Type::ResolvedPath(p) => self.path(p),
            Type::DynTrait(d) => {
                let mut parts: Vec<String> = d
                    .traits
                    .iter()
                    .map(|pt| {
                        format!(
                            "{}{}",
                            self.for_params(&pt.generic_params),
                            self.path(&pt.trait_)
                        )
                    })
                    .collect();
                if let Some(l) = &d.lifetime {
                    parts.push(l.clone());
                }
                format!("dyn {}", parts.join(" + "))
            }
            Type::Generic(s) => s.clone(),
            Type::Primitive(s) if s == "never" => "!".into(),
            Type::Primitive(s) => s.clone(),
            Type::FunctionPointer(f) => {
                format!(
                    "{}{}fn{}",
                    self.for_params(&f.generic_params),
                    header_words(&f.header),
                    self.sig_types(&f.sig)
                )
            }
            Type::Tuple(v) => match v.len() {
                1 => format!("({},)", self.ty(&v[0])),
                _ => format!(
                    "({})",
                    v.iter().map(|x| self.ty(x)).collect::<Vec<_>>().join(", ")
                ),
            },
            Type::Slice(t) => format!("[{}]", self.ty(t)),
            Type::Array { type_, len } => format!("[{}; {len}]", self.ty(type_)),
            Type::Pat { type_, .. } => self.ty(type_),
            Type::ImplTrait(bounds) => format!("impl {}", self.bounds(bounds)),
            Type::Infer => "_".into(),
            Type::RawPointer { is_mutable, type_ } => {
                format!(
                    "*{} {}",
                    if *is_mutable { "mut" } else { "const" },
                    self.ty(type_)
                )
            }
            Type::BorrowedRef {
                lifetime,
                is_mutable,
                type_,
            } => {
                let mut s = String::from("&");
                if let Some(l) = lifetime {
                    s.push_str(l);
                    s.push(' ');
                }
                if *is_mutable {
                    s.push_str("mut ");
                }
                s.push_str(&self.ty(type_));
                s
            }
            Type::QualifiedPath {
                name,
                args,
                self_type,
                trait_,
            } => {
                let args = args.as_deref().map(|a| self.args(a)).unwrap_or_default();
                let me = self.ty(self_type);
                match trait_ {
                    Some(t) if me != "Self" => format!("<{me} as {}>::{name}{args}", self.path(t)),
                    Some(_) => format!("Self::{name}{args}"),
                    None => format!("{me}::{name}{args}"),
                }
            }
        }
    }

    /// Writes a path with its generic arguments, and records its ID. The
    /// path shows only its last segment, as rustdoc's pages do: `Formatter`,
    /// not `fmt::Formatter`; the ID links the rest.
    pub fn path(&mut self, p: &Path) -> String {
        self.ids.push(p.id);
        let args = p.args.as_deref().map(|a| self.args(a)).unwrap_or_default();
        let last = p.path.rsplit("::").next().unwrap_or(&p.path);
        format!("{last}{args}")
    }

    /// Writes generic arguments: `<T, Item = U>` or `(A) -> B`.
    fn args(&mut self, a: &GenericArgs) -> String {
        match a {
            GenericArgs::AngleBracketed { args, constraints } => {
                let mut parts: Vec<String> = args.iter().map(|x| self.arg(x)).collect();
                parts.extend(constraints.iter().map(|c| self.constraint(c)));
                if parts.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", parts.join(", "))
                }
            }
            GenericArgs::Parenthesized { inputs, output } => {
                let ins: Vec<String> = inputs.iter().map(|t| self.ty(t)).collect();
                match output {
                    Some(o) => format!("({}) -> {}", ins.join(", "), self.ty(o)),
                    None => format!("({})", ins.join(", ")),
                }
            }
            GenericArgs::ReturnTypeNotation => "(..)".into(),
        }
    }

    /// Writes one generic argument.
    fn arg(&mut self, a: &GenericArg) -> String {
        match a {
            GenericArg::Lifetime(l) => l.clone(),
            GenericArg::Type(t) => self.ty(t),
            GenericArg::Const(c) => c.expr.clone(),
            GenericArg::Infer => "_".into(),
        }
    }

    /// Writes an associated item constraint: `Item = T` or `Item: Bound`.
    fn constraint(&mut self, c: &AssocItemConstraint) -> String {
        let args = c.args.as_deref().map(|a| self.args(a)).unwrap_or_default();
        match &c.binding {
            AssocItemConstraintKind::Equality(Term::Type(t)) => {
                format!("{}{args} = {}", c.name, self.ty(t))
            }
            AssocItemConstraintKind::Equality(Term::Constant(k)) => {
                format!("{}{args} = {}", c.name, k.expr)
            }
            AssocItemConstraintKind::Constraint(b) => {
                format!("{}{args}: {}", c.name, self.bounds(b))
            }
        }
    }

    /// Writes bounds joined with `+`.
    pub fn bounds(&mut self, bounds: &[GenericBound]) -> String {
        bounds
            .iter()
            .map(|b| self.bound(b))
            .collect::<Vec<_>>()
            .join(" + ")
    }

    /// Writes one bound.
    fn bound(&mut self, b: &GenericBound) -> String {
        match b {
            GenericBound::TraitBound {
                trait_,
                generic_params,
                modifier,
            } => {
                let m = match modifier {
                    TraitBoundModifier::None => "",
                    TraitBoundModifier::Maybe => "?",
                    TraitBoundModifier::MaybeConst => "~const ",
                };
                format!(
                    "{m}{}{}",
                    self.for_params(generic_params),
                    self.path(trait_)
                )
            }
            GenericBound::Outlives(l) => l.clone(),
            GenericBound::Use(args) => {
                let names: Vec<String> = args
                    .iter()
                    .map(|a| match a {
                        rustdoc_types::PreciseCapturingArg::Lifetime(l) => l.clone(),
                        rustdoc_types::PreciseCapturingArg::Param(p) => p.clone(),
                    })
                    .collect();
                format!("use<{}>", names.join(", "))
            }
        }
    }

    /// Writes `for<'a> ` before a higher-ranked bound, or nothing.
    fn for_params(&mut self, params: &[GenericParamDef]) -> String {
        if params.is_empty() {
            return String::new();
        }
        format!("for<{}> ", self.params(params))
    }

    /// Writes generic parameters without the angle brackets, leaving out the
    /// ones that `impl Trait` in an argument makes.
    fn params(&mut self, params: &[GenericParamDef]) -> String {
        params
            .iter()
            .filter_map(|p| match &p.kind {
                GenericParamDefKind::Lifetime { outlives } if outlives.is_empty() => {
                    Some(p.name.clone())
                }
                GenericParamDefKind::Lifetime { outlives } => {
                    Some(format!("{}: {}", p.name, outlives.join(" + ")))
                }
                GenericParamDefKind::Type {
                    is_synthetic: true, ..
                } => None,
                GenericParamDefKind::Type {
                    bounds, default, ..
                } => {
                    let mut s = p.name.clone();
                    if !bounds.is_empty() {
                        s.push_str(": ");
                        s.push_str(&self.bounds(bounds));
                    }
                    if let Some(d) = default {
                        s.push_str(" = ");
                        s.push_str(&self.ty(d));
                    }
                    Some(s)
                }
                GenericParamDefKind::Const { type_, default } => {
                    let mut s = format!("const {}: {}", p.name, self.ty(type_));
                    if let Some(d) = default {
                        s.push_str(" = ");
                        s.push_str(d);
                    }
                    Some(s)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Writes generic parameters as written after a name: `<S: Store>`, or
    /// nothing.
    pub fn generics(&mut self, g: &Generics) -> String {
        let p = self.params(&g.params);
        if p.is_empty() {
            String::new()
        } else {
            format!("<{p}>")
        }
    }

    /// Writes a where clause: ` where S: Store`, or nothing.
    pub fn where_clause(&mut self, g: &Generics) -> String {
        let preds: Vec<String> = g
            .where_predicates
            .iter()
            .map(|w| match w {
                WherePredicate::BoundPredicate {
                    type_,
                    bounds,
                    generic_params,
                } => format!(
                    "{}{}: {}",
                    self.for_params(generic_params),
                    self.ty(type_),
                    self.bounds(bounds)
                ),
                WherePredicate::LifetimePredicate { lifetime, outlives } => {
                    format!("{lifetime}: {}", outlives.join(" + "))
                }
                WherePredicate::EqPredicate { lhs, rhs } => {
                    let r = match rhs {
                        Term::Type(t) => self.ty(t),
                        Term::Constant(c) => c.expr.clone(),
                    };
                    format!("{} == {r}", self.ty(lhs))
                }
            })
            .collect();
        if preds.is_empty() {
            String::new()
        } else {
            format!(" where {}", preds.join(", "))
        }
    }

    /// Writes the parameter list and result of a function type:
    /// `(i32, &str) -> bool`.
    fn sig_types(&mut self, sig: &FunctionSignature) -> String {
        let ins: Vec<String> = sig.inputs.iter().map(|(_, t)| self.ty(t)).collect();
        let mut s = format!("({})", ins.join(", "));
        if let Some(o) = &sig.output {
            s.push_str(" -> ");
            s.push_str(&self.ty(o));
        }
        s
    }

    /// Writes one parameter of a function: `self`, `&mut self` or
    /// `name: Type`.
    pub fn param(&mut self, name: &str, t: &Type) -> String {
        if name == "self" {
            match t {
                Type::Generic(g) if g == "Self" => return "self".into(),
                Type::BorrowedRef {
                    lifetime,
                    is_mutable,
                    type_,
                } if matches!(&**type_, Type::Generic(g) if g == "Self") => {
                    let l = lifetime
                        .as_ref()
                        .map(|l| format!("{l} "))
                        .unwrap_or_default();
                    return format!("&{l}{}self", if *is_mutable { "mut " } else { "" });
                }
                _ => {}
            }
        }
        // rustdoc names a parameter that a pattern destructures `__arg0`.
        let name = if name.starts_with("__arg") { "_" } else { name };
        format!("{name}: {}", self.ty(t))
    }
}

/// Writes the words before `fn`: `const async unsafe extern "C" `.
pub fn header_words(h: &FunctionHeader) -> String {
    let mut s = String::new();
    if h.is_const {
        s.push_str("const ");
    }
    if h.is_async {
        s.push_str("async ");
    }
    if h.is_unsafe {
        s.push_str("unsafe ");
    }
    if let Some(abi) = abi_name(&h.abi) {
        s.push_str(&format!("extern \"{abi}\" "));
    }
    s
}

/// Names an ABI other than Rust's own, such as "C".
pub fn abi_name(abi: &Abi) -> Option<String> {
    Some(
        match abi {
            Abi::Rust => return None,
            Abi::C { .. } => "C",
            Abi::Cdecl { .. } => "cdecl",
            Abi::Stdcall { .. } => "stdcall",
            Abi::Fastcall { .. } => "fastcall",
            Abi::Aapcs { .. } => "aapcs",
            Abi::Win64 { .. } => "win64",
            Abi::SysV64 { .. } => "sysv64",
            Abi::System { .. } => "system",
            Abi::Other(o) => o,
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Makes a path type for the tests.
    fn path(name: &str, id: u32, args: Vec<Type>) -> Type {
        Type::ResolvedPath(Path {
            path: name.into(),
            id: Id(id),
            args: if args.is_empty() {
                None
            } else {
                Some(Box::new(GenericArgs::AngleBracketed {
                    args: args.into_iter().map(GenericArg::Type).collect(),
                    constraints: vec![],
                }))
            },
        })
    }

    #[test]
    fn writes_paths_references_and_tuples() {
        let t = Type::BorrowedRef {
            lifetime: Some("'a".into()),
            is_mutable: true,
            type_: Box::new(path("Vec", 1, vec![path("Book", 2, vec![])])),
        };
        let (s, ids) = Printer::one(&t);
        assert_eq!(s, "&'a mut Vec<Book>");
        assert_eq!(ids, vec![Id(1), Id(2)]);
        let (s, _) = Printer::one(&Type::Tuple(vec![Type::Primitive("u8".into())]));
        assert_eq!(s, "(u8,)");
    }

    #[test]
    fn writes_self_parameters() {
        let mut p = Printer::default();
        let t = Type::BorrowedRef {
            lifetime: None,
            is_mutable: false,
            type_: Box::new(Type::Generic("Self".into())),
        };
        assert_eq!(p.param("self", &t), "&self");
        assert_eq!(p.param("id", &path("BookId", 3, vec![])), "id: BookId");
    }
}
