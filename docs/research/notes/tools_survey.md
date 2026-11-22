# Interactive code documentation and architecture maps: tools survey (as of Nov 2026)

Scope note: about 18 tool calls. The core tools (C4, Sourcetrail, pkgsite, Sourcegraph, GitHub stack graphs, Code Wiki, DeepWiki, CodeSee, Swark, CodeCharta) have primary or near-primary sources. Understand (SciTools), CodeScene, Moose/Roassal and D2 were NOT researched in this pass. See the Gaps sections.

## 1. C4 model and Structurizr (levels, notation, zoom, dynamic diagrams)

### Takeaway
C4 is a set of hierarchical abstractions with one diagram per zoom level, plus supporting diagrams. Its notation rules suit a documentation map well: every element states its type, a short responsibility description and a technology, and every arrow is unidirectional and has a specific label. Structurizr "dynamic views" are ordered, titled flows laid over the static model. They are a ready-made model for "trace a request".

### Cited Findings
- C4 is "an easy to learn, developer friendly approach to software architecture diagramming". It has hierarchical abstractions (software systems, containers, components, code), hierarchical diagrams (system context, container, component, code), and supporting diagrams (system landscape, dynamic, deployment). It is notation- and tooling-independent. — [c4model.com](https://c4model.com/)
- Notation rules: "Every diagram should have a title describing the diagram type and scope". Every diagram needs a key or legend because "not everybody will know the notation being used". Acronyms must be understandable or explained in the key. — [C4 notation](https://c4model.com/diagrams/notation)
- Element rules: "The type of every element should be explicitly specified". Each element has "a short description" for an at-a-glance view of its key responsibilities. "Every container and component should have a technology explicitly specified." — [C4 notation](https://c4model.com/diagrams/notation)
- Relationship rules: "Every line should represent a unidirectional relationship". Every line has a label that matches its direction and intent. Avoid vague labels such as "Uses". Container-to-container lines state the technology or protocol. — [C4 notation](https://c4model.com/diagrams/notation)
- Structurizr: "Dynamic views show ordered **instances** of relationships". A dynamic view picks a subset of elements to illustrate a use case, story or feature. Each `dynamic` block lists interactions in order (for example customer -> web app -> database), has a `title`, and can use `autoLayout lr`. Because the flows live in dynamic views, the static model needs fewer relationships. One generic static relationship ("Browses and makes purchases using") sits beside specific per-feature dynamic views. — [Structurizr DSL cookbook: dynamic view](https://docs.structurizr.com/dsl/cookbook/dynamic-view/)

### Inferences
- Mapping to a Go backend: region ~ container or bounded area, package ~ component, type/function ~ code. Copy the C4 card format for each node: name, kind (package/struct/interface/func), short description from the godoc first sentence, and a "technology"-like tag (e.g. HTTP handler, store, adapter).
- Copy the "no unlabelled arrows" rule literally. Each edge type has a verb ("calls", "implements", "has field of type", "returns", "takes"). A legend is always one click away.
- Dynamic views give a "trace a request" mode: a numbered, ordered list of calls across the map, shown as either a numbered overlay or a sequence diagram. The static map stays sparse because flows live in their own views.

### Gaps
- Not verified from Structurizr docs this pass: how the Structurizr UI does double-click drill-down between views, whether it switches between collaboration and sequence rendering for dynamic views, and parallel sequences. The UI docs URL I tried returned 404.
- The C4 "code" level guidance (C4 says it is often optional or auto-generated) was not fetched.

## 2. Sourcetrail (open source, discontinued)

### Takeaway
Sourcetrail is the strongest direct precedent for an ego-centric code graph. It shows one active symbol and its direct relationships, not the whole code base. It colour-codes node and edge kinds, bundles and collapses large neighbour sets with counts, and keeps the graph and the code linked both ways on hover. It was archived in December 2021.

### Cited Findings
- Status: "This repository was archived by the owner on Dec 14, 2021. It is now read-only". The original authors archived it by the end of 2021. It was a "free and open-source cross-platform source explorer that helps you get productive on unfamiliar source code" for C, C++, Java and Python, with SourcetrailDB for custom language extensions. — [GitHub CoatiSoftware/Sourcetrail](https://github.com/CoatiSoftware/Sourcetrail)
- It became free and open source in 2019. — [InfoWorld](https://www.infoworld.com/article/2264662/sourcetrail-code-navigator-now-free-open-source.html)
- Design history: early prototypes tried to show the whole code base in one large graph. This failed because code bases grow too large for that representation. Sourcetrail moved to showing one symbol and its direct relationships, so users are not flooded with information. — [TheServerSide](https://www.theserverside.com/feature/New-IDE-tools-to-visualize-code-may-accelerate-app-dev/)
- Two-way linking: hovering over a node or edge highlights the matching code line, and hovering over code highlights the graph element. IDE plug-ins keep the visualization in sync with the editor. — [TheServerSide](https://www.theserverside.com/feature/New-IDE-tools-to-visualize-code-may-accelerate-app-dev/)
- Graph view details, from the official documentation:
  - Clicking a node activates it and updates all views.
  - Class/struct nodes show their members nested and grouped by access type, with an expand/collapse arrow and a count of hidden members.
  - **Bundle nodes** combine many nodes to reduce graph size, labelled with the bundled type and a count. Click to expand.
  - **Group nodes** show shared file or namespace.
  - **Hatched nodes** are symbols used but not defined in the indexed code.
  - Edge kinds: type use, call, include, inheritance, override, variable use.
  - Default colours: type use gray, calls yellow, variable access blue. Nodes match: types gray, functions yellow, variables blue.
  - **Bundled edges** are drawn thicker with more members, and hovering shows the count.
  - Clicking an edge shows its source locations as snippets in the Code View.
  - **Custom trails**: "To Target Symbol", "All Referenced", "All Referencing", with a depth slider, vertical or horizontal layout, and node and edge filters.
  - Back/forward history (`Z` / `Shift+Z`), bookmarks, and the context menu items Show Definition, Expand, Hide, Bookmark, and Save As Image.
  - Tooltips give the symbol type, visibility, reference count and name.
  - A `?` button opens a graph legend with example layouts.
  — [Sourcetrail DOCUMENTATION.md](https://github.com/CoatiSoftware/Sourcetrail/blob/master/DOCUMENTATION.md)
- User praise (thin evidence): an AlternativeTo review calls it "amazingly simple, and with integration for all your favorite IDEs/editors". — [AlternativeTo](https://alternativeto.net/software/sourcetrail/about)

### Inferences
- Patterns worth copying directly:
  - a one-hop ego graph by default, with depth as an explicit opt-in (slider);
  - one colour per relationship kind, reused on node kinds;
  - bundles with counts in place of 40 sibling nodes;
  - members nested inside their type node, collapsed with a count;
  - edge thickness for multiplicity;
  - a click on an edge shows the call-site snippets;
  - history and bookmarks;
  - a legend behind `?`.
- The "path to target symbol" trail answers "how does A reach B?" and is a strong feature for a Go map (e.g. handler -> store).

### Gaps
- The docs I read do not confirm the exact caller-left / callee-right layout. It is widely recalled as incoming-left, outgoing-right (horizontal layout option), but I did not verify it here.
- I found no HN thread or blog post that gives the reason for discontinuation in primary text. The linked blog post was not fetched.
- Successor forks (e.g. community forks with a Go indexer) were not checked.

## 3. Code navigation: Sourcegraph, GitHub stack graphs, VS Code, Go pkgsite

### Takeaway
Navigation tools present structure as "hover -> definition / references / implementations" panels, grouped and labelled by relationship kind, with precise data first and fuzzy search as a fallback. pkg.go.dev is the canonical Go layout: Overview, an Index that groups constructors and methods under their type, Examples, Source Files and Directories, plus tabs for Imports and Imported By with counts.

### Cited Findings
- Sourcegraph: precise navigation uses SCIP, a language-agnostic indexing protocol (`scip-go` exists). It falls back to search-based navigation when there is no precise index. The references panel groups results by repository and file, and separates definitions, references and implementations. It is truncated at more than 500 references. "Find implementations" goes from an interface to all implementations across repos, including the interfaces a struct implements. — [Sourcegraph docs: code navigation](https://sourcegraph.com/docs/code-navigation), [precise code navigation](https://sourcegraph.com/docs/code-search/code-navigation/precise_code_navigation)
- GitHub stack graphs (Dec 2021): name-binding rules are defined in a declarative DSL over Tree-sitter parse trees. Each file is analysed in isolation at push time, and cross-file resolution happens at query time by path-finding. Jump-to-definition results appear in a hover card. No per-repo config or build step is needed. Python was GA at launch. — [GitHub blog: Introducing stack graphs](https://github.blog/open-source/introducing-stack-graphs/)
- VS Code: Peek Definition and Peek References show results inline without leaving the file, with quick edits inside the peek. Ctrl+hover previews a declaration. Breadcrumbs show the file path and the symbol path, and each crumb opens a dropdown of siblings. Back and forward navigation run across files and edit locations. — [VS Code: Code Navigation](https://code.visualstudio.com/docs/editing/editingevolved)
- pkg.go.dev package page:
  - Tabs: Main, Versions, Licenses, Imports, Imported By, with counts. For `net/http`: Imports 44, Imported by 1,776,917.
  - Sidebar "Jump to": Documentation (Overview, Index, Examples, Constants, Variables, Functions, Types), Source Files, Directories.
  - In the Index, each type lists its constructor/helper functions (e.g. `NewServeMux` under ServeMux) and then its methods with receivers (`(*Client) Do`).
  - Deprecated items are marked.
  — [pkg.go.dev/net/http](https://pkg.go.dev/net/http)
- pkgsite: docs are generated from source. The first sentence is indexed and shown in search results. A build-context (GOOS/GOARCH) dropdown appears when docs differ by platform. Symbols link to their source definitions. `?` shows keyboard shortcuts. — [pkg.go.dev/about](https://pkg.go.dev/about)

### Inferences
- For a Go map, borrow the pkgsite Index grouping as the "zoomed-in package" view: types, each with its constructors and methods nested under it. Use the first doc sentence as the node subtitle. "Imported by" / "Imports" become the package-level in/out edges, shown with counts.
- Borrow the Sourcegraph panel taxonomy for the side panel of a selected symbol: Definitions, References (callers), Implementations / Implements, each a separate labelled group with counts.
- Borrow VS Code-style breadcrumbs (region > package > type > method) with sibling dropdowns, and global back/forward.

### Gaps
- VS Code Call Hierarchy and Type Hierarchy (incoming/outgoing toggle, supertypes/subtypes tree) are not on the page I fetched. I did not verify them with a source. They are known to exist as tree views, but cite the VS Code docs separately if needed.
- GitHub's current (2026) navigation UI (symbols pane, search-based vs precise per language) was not checked.
- Sourcegraph "Code Graph" product branding and its current status were not checked.

## 4. Map and wiki products: CodeSee, Swark, CodeCharta, AI wikis (DeepWiki, Google Code Wiki)

### Takeaway
AI wikis (DeepWiki, Google Code Wiki) present "what this does / how it fits" as structured prose pages with generated architecture, class and sequence diagrams. Every section links to the code, and a chat is grounded in the wiki. Their weakness is trust. CodeSee was acquired and shut down. Swark is a thin LLM-to-Mermaid generator. CodeCharta is a metrics city map, so it is mostly off-target for understanding.

### Cited Findings
- Google Code Wiki (public preview, Nov 13 2025):
  - "continuously updated, structured wiki for every repository", regenerated after each change;
  - "Every wiki section and chat answer is hyper-linked directly to the relevant code files and definitions";
  - "automatically generates always-current architecture, class, and sequence diagrams";
  - a Gemini chat uses the wiki as context;
  - a Gemini CLI extension for private repos was "coming soon".
  — [Google Developers Blog](https://developers.googleblog.com/introducing-code-wiki-accelerating-your-code-understanding/)
- Code Wiki carries the disclaimer "Gemini can make mistakes, so double-check it". — [The Register](https://www.theregister.com/2025/11/17/google_previews_code_wiki/)
- DeepWiki (Devin): auto-indexes repos into wikis "with architecture diagrams, links to sources, and summaries". Ask Devin answers questions using the wiki plus code search. A `.devin/wiki.json` file steers it with a title and purpose per page. A free public-repo version and an MCP server exist. — [Devin docs: DeepWiki](https://docs.devin.ai/work-with-devin/deepwiki)
- CodeSee: acquired by GitKraken on May 14, 2024, which folded "code visualization ... and intuitive code and function maps" into its DevEx platform. — [GitKraken press release](https://gitkraken.com/press/gitkraken-acquires-codesee-launches-devex-platform), [SD Times](https://sdtimes.com/software-development/gitkraken-acquires-codesee-launches-new-devex-platform/)
- A co-founder post says CodeSee would be "closing its doors and shutting down commercial operations on Thursday, Feb 22nd". The year is not stated, but Feb 22 fell on a Thursday in 2024. Treat this as a likely 2024 shutdown that is not fully confirmed by a primary notice. — [LinkedIn snippet](https://sk.linkedin.com/in/daniel-ivanyi-16845458)
- Swark (VS Code extension): gathers source files, builds a prompt, calls GitHub Copilot through the VS Code Language Model API, and writes a Mermaid architecture diagram to `swark-output/`. The LLM can produce cyclic or invalid Mermaid. — [Swark Marketplace](https://marketplace.visualstudio.com/items?itemName=swark.swark), [GitHub](https://github.com/jstuart0/swark)
- CodeCharta (MaibornWolff, BSD-3): a 3D "code city". Files are buildings and folders are districts, with metrics mapped to area, height and colour. Metrics come from its shell (ccsh), and it runs in a web studio. — [GitHub maibornwolff/codecharta](https://github.com/maibornwolff/codecharta), [docs](https://maibornwolff.github.io/codecharta/docs/visualization)

### Inferences
- The AI-wiki pattern to copy: a human-readable "what this does / how it fits" paragraph for each region and package, where every noun links to a symbol on the map. Generated sequence diagrams for key flows. Mark generated text as such, given the trust concerns.
- A per-page "title + purpose" steering file (DeepWiki) suggests letting maintainers pin or curate region descriptions, so they are not purely generated.
- The city / treemap metaphor (CodeCharta) helps show size and containment (region > package) but not relationships. It could be a background layout for the zoomed-out level at most.
- CodeSee's fate, and Sourcetrail's, suggest that standalone code-map products struggle commercially. That is no evidence against the UI patterns.

### Gaps
- Not researched this pass: Understand (SciTools) graphs (butterfly/call trees, UML class views), CodeScene ownership/history context, Moose/Roassal, D2 and Mermaid architecture syntax specifics.
- CodeSee's map UI details (directory boxes with dependency arrows, "tours") were not verified.
- The independent accuracy of DeepWiki or Code Wiki output is not quantified in the sources found.

## 5. Cross-tool patterns and known failures (hairballs, too many edges, unlabelled arrows)

### Takeaway
Every successful tool limits what is drawn at once: one hop from a focus symbol (Sourcetrail), one zoom level per diagram (C4), grouped lists and not graphs for references (Sourcegraph, pkgsite). Whole-code-base node-link graphs consistently fail as hairballs. The fixes in the sources are bundling with counts, collapsible trees, depth limits, filtering, edge weighting, matrix views, and labelled, legend-backed edges.

### Cited Findings
- Sourcetrail's whole-code-base prototype failed, and the product moved to a one-level view. — [TheServerSide](https://www.theserverside.com/feature/New-IDE-tools-to-visualize-code-may-accelerate-app-dev/)
- Edges grow roughly quadratically with nodes. Mitigations include k-core filtering and edge bundling. — [Medium: Hairball Graph Problem](https://medium.com/@harikrishnank497/hairball-graph-problem-a8ce62d324d5) (practitioner post)
- Square's DependenTree renders large dependency graphs as a collapsible tree so the user sees "just as much information as they want", and caps traversal depth. — [Square: DependenTree](https://developer.squareup.com/blog/dependentree-graph-visualization-library)
- Visual Studio recommended a dependency matrix to find dense areas when the node-link graph is unreadable. — [Microsoft Learn (VS2010)](https://learn.microsoft.com/en-us/previous-versions/visualstudio/visual-studio-2010/ee847415(v=vs.100))
- Facebook's PHP code base visualisation counted a module dependency the same with 1 call or 1,000, and flagged this as a key flaw. In effect this argues for edge weighting. — [Engineering at Meta](https://engineering.fb.com/developer-tools/visualizing-facebook-s-php-codebase)
- Many tiny connections in code bases "don't matter" and are noise. — [DEV post](https://dev.to/jenilsavani_/i-built-a-codebase-graph-viewer-to-understand-my-systems-true-shape-1laf) (practitioner)
- Unlabelled or vague arrows: C4 requires a specific label on every arrow and bans "Uses". — [C4 notation](https://c4model.com/diagrams/notation)

### Inferences (a design checklist for the Go map)
1. **Focus + one hop:** selecting a symbol shows callers on the left, the item in the middle, and callees on the right. Implementations and field types go in labelled lanes or groups. Sourcetrail and C4 support one-hop focus; the left-to-right lane layout is a common convention not verified for Sourcetrail here.
2. **Group neighbours by relationship kind** (calls / called by / implements / implemented by / field types / params / returns), each lane with a count. Collapse big lanes into a bundle node "+37 callers".
3. **Colour and verb per edge kind**, always labelled, with a legend behind `?`.
4. **Edge weight** (thickness or count badge) for package-to-package edges aggregated from symbol edges.
5. **Semantic zoom:** region (C4 container-like card with description) > package (pkgsite-style index: types with constructors and methods nested) > symbol (ego graph plus doc plus source link).
6. **Breadcrumbs** with sibling dropdowns, back/forward history, and bookmarks.
7. **Hover previews** (doc first sentence and signature). Click an edge to show the call-site snippets. "Open source" links everywhere.
8. **Trace / path mode:** "path from A to B" (Sourcetrail's To Target Symbol) and curated, numbered request flows (Structurizr dynamic views or a generated sequence diagram).
9. **Fallback lists:** when a lane exceeds about 20 to 50 nodes, show a grouped list (Sourcegraph style), not more nodes. Sourcegraph truncates at 500.

### Gaps
- I found no controlled user study comparing ego-graph vs whole-graph code maps for comprehension. The evidence is designer testimony and practitioner posts.
- Hover-preview and back/forward conventions in Sourcegraph's current UI were not verified directly.
