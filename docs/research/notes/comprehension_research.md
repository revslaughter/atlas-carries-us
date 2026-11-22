# Program comprehension and software visualization research for a zoomable Go documentation map

Source key (full texts I read are marked [read]; the rest are abstracts or records):
- SWM = Storey, Wong, Müller, "How do program understanding tools affect how programmers understand programs?", WCRE 1997 / Sci. Comp. Prog. 2000. [read] https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf
- SFM = Storey, Fracchia, Müller, "Cognitive design elements to support the construction of a mental model during software exploration", JSS 44(3), 1999, pp. 171-185. [read] https://plg.uwaterloo.ca/~migod/846/papers/storey-jss.pdf
- LM-Reach = LaToza & Myers, "Developers ask reachability questions", ICSE 2010. [read] https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/LaToza2010ICSE.pdf
- Reacher = LaToza & Myers, "Visualizing call graphs", VL/HCC 2011. [read] https://people.cs.gmu.edu/~tlatoza/papers/vlhcc2011.pdf
- LaToza-IEEE = LaToza, "Information Needs: Lessons for Programming Tools", IEEE Software Nov/Dec 2020. [read] https://people.cs.gmu.edu/~tlatoza/papers/InformationNeeds-LessonsForProgrammingTools.pdf
- Ko07 = Ko, DeLine, Venolia, "Information needs in collocated software development teams", ICSE 2007. [read; the PDF's digits were lost in extraction] https://faculty.washington.edu/ajko/papers/Ko2007InformationNeeds.pdf
- IFT = Lawrance, Bogart, Burnett, Bellamy, Rector, Fleming, "How programmers debug, revisited: an information foraging theory perspective", IEEE TSE 39(2), 2013. [read] https://web.engr.oregonstate.edu/~burnett/Reprints/TSE-IFT-2013-asprinted.pdf
- CB = Bragdon et al., "Code Bubbles: a working set-based interface for code understanding and maintenance", CHI 2010. [read] https://www.cs.ucf.edu/~jjl/pubs/p2503-bragdon.pdf
- DC = DeLine, Bragdon, Rowan, Jacobsen, Reiss, "Debugger Canvas: industrial experience with the Code Bubbles paradigm", ICSE 2012. [read] https://microsoft.com/en-us/research/wp-content/uploads/2016/02/p1064-213-deline.pdf
- CodeMap = Cherubini, Venolia, DeLine, "Building an ecologically valid, large-scale diagram to help developers stay oriented in their code", VL/HCC 2007. [read] https://microsoft.com/en-us/research/wp-content/uploads/2016/02/codemap_vlhcc07.pdf
- WB = Cherubini, Venolia, DeLine, Ko, "Let's go to the whiteboard: how and why software developers use drawings", CHI 2007. [read abstract] https://faculty.washington.edu/ajko/papers/Cherubini2007DevelopersDrawCode.pdf
- Roehm = Roehm, Tiarks, Koschke, Maalej, "How do professional developers comprehend software?", ICSE 2012. [read] https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/Roehm2012ICSE.pdf

## 1. What comprehension models say, and which developer questions are most frequent and hardest

### Takeaway
The classic models (Brooks top-down with beacons, Pennington/Shneiderman bottom-up chunking, Letovsky's opportunistic inquiry episodes, von Mayrhauser and Vans' integrated model, Littman's systematic vs as-needed) agree that developers switch strategies all the time and mostly read only what the task needs. A tool must support all of them, with cheap switching between levels. Field studies show that the hardest and most frequent questions are high level: "what are the implications of this change", "why was this implemented this way", "how does control/data get from A to B". These are path and rationale questions, not structure questions.

### Cited Findings
**Models (theory, mostly 1970s-1990s lab studies, summarised in SWM)**
- Brooks: programs are understood top-down by rebuilding domain knowledge and mapping it to code. A global hypothesis is refined into secondary hypotheses, and these are verified or rejected by the presence or absence of *beacons* (cues) — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf)
- Example beacon: a swap inside nested loops signals a sort. Variable and other names are also beacons. IFT holds that beacons are "closely related to cues in information foraging theory" — [IFT](https://web.engr.oregonstate.edu/~burnett/Reprints/TSE-IFT-2013-asprinted.pdf)
- Soloway and Ehrlich: top-down is used when the program or program type is familiar. Experts recognise programming plans and use conventions — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf)
- Pennington: programmers first work bottom-up from statement and control-flow information (the program model), and chunk it into higher-level abstractions (later the situation model, i.e. data-flow and functional abstractions) — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf); [SFM](https://plg.uwaterloo.ca/~migod/846/papers/storey-jss.pdf)
- Letovsky: programmers are opportunistic processors who use bottom-up or top-down cues. The model has a knowledge base, a mental model and an assimilation process. *Inquiry episodes* (ask a question, conjecture an answer, search code or docs to verify) are central. He classified questions as why, how, what (and others), and programmers "pick a method which is likely to yield an answer with little effort" — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf); [IFT](https://web.engr.oregonstate.edu/~burnett/Reprints/TSE-IFT-2013-asprinted.pdf)
- Littman et al.: *systematic* (read all of it, trace control and data flow) vs *as-needed* (only task-relevant code). As-needed is more common and the only feasible one for large programs, but it misses interactions and leads to more mistakes — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf)
- von Mayrhauser and Vans: programmers switch often between the top-down, program and situation models, building understanding at several levels at the same time — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf)
- From SWM's 30-participant study: tools should "support a combination of comprehension strategies; provide ways to easily enter and effortlessly switch between strategies while solving a task; and reduce cognitive overhead as the program is explored." Tools should ease the preferred strategy and not impose one — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf)

**Questions developers ask (empirical)**
- Sillito, Murphy, De Volder (FSE 2006): two qualitative studies of developers doing change tasks on medium-to-large code. They catalogued 44 question types in 4 categories by scope — [UBC record](https://open.library.ubc.ca/collections/831/items/1.0052042); [dblp](https://dblp.org/rec/conf/sigsoft/SillitoMV06). The categories are: Finding focus points; Building on (expanding) those points; Understanding a subgraph; Questions over groups of subgraphs — [TSE 2008 outline](https://typeset.io/pdf/asking-and-answering-questions-during-a-programming-change-21o3wehtmb.pdf). Developers refine high-level questions (implications of a change) into low-level ones the IDE can answer. Many difficulties came from separating task-relevant results from the many irrelevant ones that tools return. Of Sillito's questions judged poorly supported by tools, LaToza and Myers estimate 52% are reachability questions — [LM-Reach](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/LaToza2010ICSE.pdf)
- LaToza & Myers, "Hard-to-answer questions about code" (PLATEAU 2010): a Microsoft survey in which 179 respondents listed 371 hard-to-answer questions — [search summary of PLATEAU paper](https://ecs.wgtn.ac.nz/foswiki/pub/Events/PLATEAU/2010Program/plateau10-latoza-slides.pdf). LaToza's 2020 summary says this work "found that understanding design rationale, understanding implementations, and debugging are the most common challenges". Example hard questions: "In what situations or user scenarios is this called?", "How do calls flow across process boundaries?", "What is the original source of these data?", "How did this runtime state occur?" — [LaToza-IEEE](https://people.cs.gmu.edu/~tlatoza/papers/InformationNeeds-LessonsForProgrammingTools.pdf)
- LaToza & Myers, reachability (ICSE 2010): three studies. (1) In a lab study with 13 developers, half of the bugs developers inserted were associated with reachability questions. (2) In a survey of 460 professionals, developers asked reachability-type questions more than 9 times a day, 82% rated at least one of 12 questions at least somewhat hard, and 29% rated at least one very hard. (3) In a field study with 17 developers, 9 of the 10 longest activities were associated with reachability questions — [LM-Reach](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/LaToza2010ICSE.pdf)
- In the same survey, difficulty correlated with frequency (r = .35, p < .0001). "What are the implications of this change?" was both the most frequent and the hardest: half asked it at least twice a day, and over 60% found it usually at least somewhat hard. Only 16% found "How are instances of these classes or data structures created and assembled?" hard. "Generally, the most frequent and difficult questions were the most high level." Experience and time in the codebase did *not* make these questions rarer or easier (r ≈ -.01 to -.07, n.s.) — [LM-Reach](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/LaToza2010ICSE.pdf)
- The 12 surveyed questions include: "In what situations is this method called?", "How is control getting (from that method) to this method?", "What parts of this data structure are accessed in this code?", "What is the correct way to use or access this data structure?", "When investigating some feature, how is it implemented?" — [LM-Reach](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/LaToza2010ICSE.pdf)
- In a lab study on jEdit, developers spent 68% of their time investigating code (22% debugger, 46% reading/following calls/browsing) and only 14% editing — [LM-Reach](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/LaToza2010ICSE.pdf). Another cited study found developers spend 35% of their time navigating code, more than reading (20%) or editing (20%) — [LaToza-IEEE](https://people.cs.gmu.edu/~tlatoza/papers/InformationNeeds-LessonsForProgrammingTools.pdf)
- Ko, DeLine, Venolia (ICSE 2007): 17 Microsoft developers observed, 21 information types identified. The most frequently sought information was awareness (artifacts, coworkers). The most often deferred searches were "knowledge about design and program behavior, such as why code was written a particular way, what a program was supposed to do, and the cause of a program state". These were often deferred because the only source was an unavailable coworker. "Why was this code implemented this way?" was among the most frequently unsatisfied needs — [Ko07](https://faculty.washington.edu/ajko/papers/Ko2007InformationNeeds.pdf)
- Roehm et al. (ICSE 2012): 28 professionals from 7 companies. Developers "try to avoid program comprehension". They use recurring structured strategies by context. Standards and experience help. Face-to-face communication is preferred to documentation. No use of state-of-the-art comprehension tools was observed. 21 participants said source code and inline comments are trusted more than documentation (only 4 named documentation as their main source). Standard naming and a common architecture helped developers locate code — [Roehm](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/Roehm2012ICSE.pdf)

### Inferences
- The map's primary job should be to answer path questions ("who calls this / how does a request get from handler to store / where does this value come from") and rationale questions. A static package tree answers neither. Directed traces along calls and data (a request path through regions) are likely worth more than a full graph.
- Because as-needed reading dominates and developers "avoid comprehension", each node should answer "what is this for" in one line before the user opens anything (a beacon/scent role).
- Sillito's four categories map directly onto map interactions: find a focus point (search), expand it (neighbours), understand a subgraph (a feature slice or call chain), and compare across subgraphs (e.g., all implementers of an interface).
- Go conventions (package names, interface names, `New*` constructors) act as beacons. Standard structure aids comprehension (Roehm), so surfacing conventional roles (constructor, handler, store, interface implementer) is likely useful.

### Gaps
- I did not read the full PLATEAU 2010 hard-to-answer paper, so I have no per-category counts for its 371 questions beyond LaToza's 2020 summary.
- I did not extract the full list of Sillito's 44 questions. The TSE 2008 paper (Sillito, Murphy, De Volder, TSE 34(4), 434-451) holds them.
- Ko07's percentages and times per unsatisfied need were lost in PDF extraction. Only the ranking text was recovered.

## 2. Storey, Fracchia & Müller (1999): every cognitive design element and what it implies

### Takeaway
SFM give 14 elements in two branches: improve program comprehension (E1-E7) and reduce the maintainer's cognitive overhead (E8-E14). Almost every element maps onto a feature of a zoomable map: relations, abstraction, overview at levels, hypothesis support, linked views, directional and arbitrary navigation, focus, path history, next-step options, low UI overhead, and good presentation.

### Cited Findings
All quotes and paraphrases below are from [SFM](https://plg.uwaterloo.ca/~migod/846/papers/storey-jss.pdf).

**Improve program comprehension: enhance bottom-up comprehension**
- **E1 Indicate syntactic and semantic relations between software objects.** Give immediate, visible access to the lowest-level units (code or icons). Syntactic and semantic relations (data flow, function) must be "clearly visible and easily accessible". Graphs of nodes and arcs with direct links to the source are the usual approach.
- **E2 Reduce the effect of delocalized plans.** Code for one plan is spread across files, and reading it causes frequent switching and "a feeling of disorientation". Examples: Whorf links views by giving the same object the same colour in every view, and highlights code in a delocalized plan; program slicing.
- **E3 Provide abstraction mechanisms.** Building hierarchical abstractions is "the hardest part of bottom-up comprehension", yet many tools only show prefabricated abstractions. "Maintainers might understand the software better through abstractions they created themselves, rather than through the prefabricated and less trusted abstractions". Users should be able to aggregate, label and document their own groupings (collapse a subgraph into a composite node) and filter (hide) objects.

**Enhance top-down comprehension**
- **E4 Support goal-directed, hypothesis-driven comprehension.** Let users document hypotheses, link them to code and docs, and refine them into sub-hypotheses. Postponed or discarded hypotheses are useful documentation later (TLES, Hy+ queries for design patterns).
- **E5 Provide overviews of the system architecture at various levels of abstraction.** Nested graphs (SHriMP, Landscape views, Continuous Zoom) let any level be shown or elided. Rigi's hierarchy, built bottom-up once, is then explored top-down by later maintainers.

**Integrate bottom-up and top-down**
- **E6 Support the construction of multiple mental models (domain, situation, program).** Mental models differ by maintainer. Provide multiple views: call graphs, variable use, source, requirements, design specs, slices, metrics, execution.
- **E7 Cross-reference mental models.** Link views, for example by highlighting the same object in all views or synchronised views. Mapping between models is hard because links can be one-to-many (one function description maps to chunks in many files).

**Reduce cognitive overhead: facilitate navigation**
- **E8 Provide directional navigation.** Read code and docs sequentially, follow data-flow and control-flow links, traverse hierarchies, and follow user-defined links (hypertext between code and docs).
- **E9 Support arbitrary navigation.** Reach places not linked from the current one: bookmarks ("dog-earing"), saved views or snapshots for rapid return, and search.

**Provide orientation cues**
- **E10 Indicate the maintainer's current focus.** Show the artifacts of immediate interest *and* their context. Highlighting fails in large graphs. Fisheye techniques give more space to important information.
- **E11 Display the path that led to the current focus.** Histories and breadcrumb trails, and highlighting visited nodes in an overview. "Recording why a maintainer is interested in a particular software object is very important" (the hypothesis or the change being made), and tools rarely support it.
- **E12 Indicate options for further exploration.** Make the user aware of what can be explored next (visible hyperlinks, graph edges as navigation cues), not just make it possible.

**Reduce disorientation**
- **E13 Reduce additional effort for user-interface adjustment.** Functionality should be "visible and relevant". Discontinuity between views causes disorientation. Animate zoom in and out and morph between layouts.
- **E14 Provide effective presentation styles.** Use layouts and graph presentation that make large information legible. (The section is cut short in my extract.)

**Evidence on these elements from SHriMP's user study** ([SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf); 30 participants, mostly students, comparing Rigi, SHriMP and SNiFF+ on a small Monopoly C program)
- SHriMP better supported switching between top-down and bottom-up. Users zoomed out "when a user paused to rethink a strategy, to obtain more context, or to switch between subtasks".
- "The naming of subsystem nodes was critical to the effectiveness of Rigi and SHriMP". A misleading subsystem name ("Buying&Selling" rather than "Transactions") sent users elsewhere.
- The biggest complaint was the number of visible arcs. Two users proposed hiding all arcs by default and showing arcs of a type, or of selected nodes, on request, to better support as-needed comprehension.
- Multiple windows in Rigi and SNiFF+ "often disoriented the users", who had to integrate implicit relations across windows.
- Questionnaire (5-point scale, Z tests): Rigi was rated harder to use than SHriMP (p = .004) and SNiFF+ (p = .02). There was no significant difference between SHriMP and SNiFF+.

### Inferences
- E3 (user-made abstractions) and E4/E11 (record *why* the user is looking) are the elements least supported by doc tools. A map could let users pin, name and annotate a working set or "tour", and keep a breadcrumb trail.
- E1 and E12 together argue for edges that are discoverable but not drawn until asked for (SWM's "arcs hidden by default" finding), plus a visible list of relation types available from the focus node.
- E13 implies smooth, animated zoom with a stable layout (no re-layout jumps between levels).
- Naming is the main cue at the region and package level. Region labels should describe a purpose, not a directory.

### Gaps
- The E14 text was truncated in my extraction. The heading is certain but the specific recommendations are not.
- SFM is a framework derived from theory and from observation. It has no controlled validation of each element.

## 3. Information foraging theory (IFT) applied to programming: scent, patches, cues

### Takeaway
Developers navigate code like foragers following scent: they choose links whose proximal cues (mostly words in identifiers, comments and labels) look most related to their goal. A model built on word similarity and the link topology predicts navigation better than models built on stated hypotheses. So for a map, labels, previews and link text are the main design lever.

### Cited Findings
- IFT constructs as defined for code: **prey** (what the developer seeks, e.g. the fix); **information patches** (localities in code, documents or displays that may hold prey); **proximal cues** ("words, objects, and perceptible runtime behaviors in the programming environment that suggest scent relative to the distal prey"; e.g. the identifiers in `System.out.println(someData)`); **scent** (word similarity between the bug report and the cues); **topology** (a graph of navigable elements and links, including class labels in a hierarchy browser); **link** (a connection between two nodes that costs navigation effort) — [IFT](https://web.engr.oregonstate.edu/~burnett/Reprints/TSE-IFT-2013-asprinted.pdf)
- Study: 10 professional programmers debugged a real open-source program. Their verbalisations "far more often concerned scent-following than hypotheses". The executable model (PFIS, using TF-IDF word similarity and spreading activation across the topology, with camel-case identifiers split and stemmed) "predicted programmer navigation behavior more accurately than comparable models that did not consider information scent" — [IFT](https://web.engr.oregonstate.edu/~burnett/Reprints/TSE-IFT-2013-asprinted.pdf)
- For large programs "the only realistic strategy is to focus on relevant parts of the code and to navigate among them". Programmers accept partial comprehension and "follow scents that involve words related, directly or indirectly, to the bug report" — [IFT](https://web.engr.oregonstate.edu/~burnett/Reprints/TSE-IFT-2013-asprinted.pdf)
- CHI 2008 PFIS: performed close to aggregated human navigation and better than individual programmers' choices. A later IBM lab study with 12 participants supports PFIS, PFIS2 and a scent-only model — [Lawrance et al. CHI 2008 PDF](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/Lawrance2008CHI.pdf); [dblp](https://dblp.uni-trier.de/rec/conf/chi/LawranceBBR08.html) (summary-level only; not read in full)
- LaToza frames choosing "which of the many call relationships between methods to navigate" as an information-foraging problem — [LaToza-IEEE](https://people.cs.gmu.edu/~tlatoza/papers/InformationNeeds-LessonsForProgrammingTools.pdf)

### Inferences
- Every node and edge label is a proximal cue. Package and type names alone are weak scent when names are terse (common in Go: `svc`, `repo`, `pkg`). A one-line purpose taken from the Go doc comment (the first sentence, per Go convention) next to each name raises scent cheaply.
- Hover previews (signature plus first doc sentence) let users sample a patch before paying the navigation cost. CB also used hover previews ([CB](https://www.cs.ucf.edu/~jjl/pubs/p2503-bragdon.pdf), Fig. 1 caption).
- Edge labels should name the relation and the target ("calls store.SaveOrder", "implements Repository"), not just draw a line, so that a link carries scent about its destination.
- Search over identifiers *and* doc text, with camel-case splitting, matches how scent is computed in PFIS.
- Region (patch) boundaries should follow what developers look for (features, request flows), so that a patch is likely to contain its prey.

### Gaps
- I did not read Piorkowski or Fleming et al. (e.g., Fleming et al. TOSEM 2013 "An information foraging theory perspective on tools for debugging, refactoring, and reuse tasks"), which hold the more detailed tool-design implications. [40] in IFT points to an in-depth treatment that I did not retrieve.

## 4. Evidence on focus+context, fisheye, semantic zoom, ego/neighbourhood and canvas views for code

### Takeaway
Measured wins come from tools that show a task's *working set* or *path* together on one canvas (Code Bubbles: 33% faster, much less navigation time; Reacher: over 5 times more successful on control-flow questions), not from whole-system pictures. In SHriMP, users chose pan+zoom over fisheye distortion. Large static maps of the whole system went unused, except by newcomers onboarding.

### Cited Findings
- **SHriMP (nested graph, pan+zoom, fisheye), 30 participants**: fisheye views "were thought to be useful" but users "often would not use the fisheye view feature. Instead, they zoomed in to see detail and then zoomed out when more context was desired". The authors' explanations: hyperlinked code in nodes already gave context, pan+zoom was fast ("one click away"), the fisheye did not support multiple focal points, and users wanted to expand several non-adjacent nodes — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf)
- **Code Bubbles (CHI 2010)**: 20 Brown CS students (graduate and senior undergraduate, mean 3.85 years of Eclipse) against Eclipse 3.4.2. Tasks: bug fixes in a drawing app (Task 1: 4 classes, 1,017 lines, 43 relevant methods). Code Bubbles users completed both tasks 33.2% faster. Task 1 was significantly faster, and Task 2 was not significantly faster. Navigation time was 3.5 min (SD 1.6) against 11.6 min (SD 3.08) in Eclipse. Bubbles show code at the granularity of single methods, with automatic non-overlapping layout, groups and hover preview. A qualitative study with 14 professionals was positive — [CB](https://www.cs.ucf.edu/~jjl/pubs/p2503-bragdon.pdf)
- The ICSE 2010 Code Bubbles paper reports 23 professional developers with "a high level of excitement, interest, and potential benefits" (opinion data) — [ICSE 2010 record](https://www.eecs.ucf.edu/~jjl/pubs/ICSE0289.pdf)
- **Debugger Canvas (ICSE 2012, Microsoft, shipped for Visual Studio)**: a pan-and-zoom canvas of bubbles along the call path during debugging. A RITE usability study had 10 participants. A survey of 341 internal users got 99 responses: 72 had used it, 53 still used it and 19 had abandoned it. Reasons for abandoning were mostly bugs, performance or platform (14 responses), features not discovered (6), and "concept didn't work for me" or wanted features (about 7). 9 of 11 interviewees named useful situations. The most common (6 of 11) was "debugging long and complicated call paths", and 5 of 11 named large or unfamiliar codebases ("code is laid out to reflect the call paths rather than file organization"). The lesson: the paradigm scales to customer code and is best as a mode inside the existing IDE, not a replacement — [DC](https://microsoft.com/en-us/research/wp-content/uploads/2016/02/p1064-213-deline.pdf); [MSR page](https://www.microsoft.com/en-us/research/publication/debugger-canvas-industrial-experience-with-the-code-bubbles-paradigm/)
- **Reacher (VL/HCC 2011)**: an interactive call-graph view that lets developers *search along* control flow rather than traverse it by hand. It encodes ordering, repetition (loops), choice and type membership. In a lab study with 12 participants answering control-flow questions, participants with Reacher "were over 5 times more successful in significantly less time" than with an existing IDE, and remained oriented. All preferred it — [Reacher](https://people.cs.gmu.edu/~tlatoza/papers/vlhcc2011.pdf)
- Motivation for Reacher: developers "can spend tens of minutes answering a single question, get lost and disoriented, and erroneously make assumptions, causing bugs" when they traverse call graphs by hand — [Reacher](https://people.cs.gmu.edu/~tlatoza/papers/vlhcc2011.pdf)
- **Code map on the wall (VL/HCC 2007)**: a large, spatially stable, paper code map was co-designed with a Microsoft team and posted for about a month, updated daily. There was "little evidence of interaction", except that two new hires used their MiniMaps "extensively to support their 'onboarding' process" and annotated them (adding the DLL names). Reasons for non-use: "too few or too many details in relation to the scope of the discussion"; the content was not dynamic (one wanted the call graph between classes rather than inheritance); the layout was static. The map was at the ergonomic limit (5-point font for members). The authors present it as "a cautionary tale" — [CodeMap](https://microsoft.com/en-us/research/wp-content/uploads/2016/02/codemap_vlhcc07.pdf)
- LaToza & Myers sum up the code map work: diagrams "designed to be useful for all possible tasks... had both too much and too little information". Diagrams that give "concise and targeted answers to situation-relevant questions were more likely to be useful than general-purpose diagrams". UML tool studies recommended better support for selecting task-relevant items in reverse-engineered views — [LM-Reach](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/LaToza2010ICSE.pdf)
- Whiteboard study (CHI 2007; interviews plus a survey at Microsoft): most diagrams were transient. "Diagrams that documented design decisions were often externalized in these temporary drawings and then subsequently lost" — [WB](https://faculty.washington.edu/ajko/papers/Cherubini2007DevelopersDrawCode.pdf)
- Roehm et al. saw no use of state-of-the-art comprehension tools among 28 professionals — [Roehm](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/Roehm2012ICSE.pdf)

### Inferences
- Design priority: (1) an ego/neighbourhood view around a focus node, with relation-typed expansion; (2) a path/trace mode (A to B along calls) that answers reachability questions; (3) semantic zoom with animated pan+zoom as the main context mechanism. Fisheye distortion is low priority, and if built it needs multiple foci.
- A whole-system overview is valuable mainly for onboarding (CodeMap's new hires) and for reorientation (SWM's zoom-out to rethink). Keep it, but make it a reorientation layer, not the main work surface.
- Let users build a persistent working set (Code Bubbles groups) that can be named and shared; this also covers SFM's E3/E11.
- Content must be dynamic: let the user choose which relation (calls vs implements vs field types) is drawn, because the needed relation depends on the task (CodeMap finding).

### Gaps
- I found no primary evaluations of Code Canvas (DeLine and Rowan, "Code Canvas: zooming towards better development environments", ICSE 2010 NIER), Relo (Sinha, Karger, Miller 2005) or Sourcetrail in this pass. As far as I know, Code Canvas and Relo are design papers with little or no controlled evaluation, and Sourcetrail has no peer-reviewed evaluation. This is unverified.
- I found no controlled study isolating semantic zoom (level-dependent content) from plain geometric zoom for code.

## 5. Which metrics and visual encodings are noise vs useful

### Takeaway
The evidence does not support decorating a comprehension map with size metrics by default. The cited studies point to clutter (too many arcs, too much detail) as the main harm, and to names and task-relevant relations as the main help. Metric-encoded visualisations (CodeCity) help on tasks that are themselves about metrics or design smells.

### Cited Findings
- In SHriMP, the biggest concern was "the large number of visible arcs". Users proposed hiding arcs by default and showing them on request — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf)
- Subsystem naming, not geometry, was "critical" — [SWM](https://www.st.cs.uni-saarland.de/edu/empirical-se/2006/PDFs/storey98.pdf)
- A general-purpose map with "too few or too many details" relative to the task went unused — [CodeMap](https://microsoft.com/en-us/research/wp-content/uploads/2016/02/codemap_vlhcc07.pdf)
- SFM list "software statistics (metrics)" as one textual view among many to support multiple mental models (E6), not as a primary encoding — [SFM](https://plg.uwaterloo.ca/~migod/846/papers/storey-jss.pdf)
- CodeCity (Wettel, Lanza, Robbes, ICSE 2011): 41 participants in 4 locations, 3 countries, academia and industry. Compared with Eclipse+Excel, the city metaphor (buildings sized by class metrics) gave "a statistically significant increase in terms of task correctness and decrease in task completion time" — [ICSE 2011 abstract page](https://2011.icse-conferences.org/content/software-systems-cities-controlled-experiment.html); [USI tech report](https://www.inf.usi.ch/faculty/lanza/PUBS/M/Wett2010a.pdf)
- Developers found "How are instances ... created and assembled?" easy (16% hard). Structural questions are less of a pain point than path and implication questions — [LM-Reach](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/LaToza2010ICSE.pdf)

### Inferences
- LOC-based node sizing tells the reader where code mass is, which is not what they ask about. Keep metrics out of default encoding. A badge such as "N callers / M implementers" serves navigation (it scents a link) and is a better use of numbers than size.
- CodeCity's gains likely reflect metric-oriented tasks such as finding god classes. This is my inference: I could not confirm the task list, so it does not transfer to "what does this do" questions without evidence.
- Visual weight should encode relevance to the current focus (distance in the graph, on the current path), not static size.

### Gaps
- I could not retrieve CodeCity's exact effect sizes or task list; the PDF did not download. I recall about +24% correctness and -12% time, but this is unverified.
- I found no study that directly tests LOC encoding against no encoding for comprehension. Merino et al.'s systematic reviews of software visualization evaluations (2018) would be the place to look; I did not retrieve them.

## 6. What context helps a reader learn "why this exists"

### Takeaway
"Why" (rationale, intended behaviour) is among the most sought and least satisfied information. It usually lives in people's heads or in lost whiteboard sketches. Developers trust code and inline comments more than separate documents. Rationale is most usable when it is attached to the code it explains, in both directions.

### Cited Findings
- "Why was this code implemented this way?" and "What is the program supposed to do?" were among the most often deferred or unsatisfied needs. The usual source was an unavailable coworker — [Ko07](https://faculty.washington.edu/ajko/papers/Ko2007InformationNeeds.pdf)
- Developer quote in Ko07: "what never gets written in the spec is why we de[cided]..." — [Ko07](https://faculty.washington.edu/ajko/papers/Ko2007InformationNeeds.pdf)
- Understanding design rationale is among the most common hard challenges. Design rules "vanish, failing to be documented or updated and remaining tacit in developers' heads". IDEs should make rationale "more explicit and better connected to the code" and "bidirectionally connected to the code", rather than in documents disconnected from the code "or in comments disconnected from all of its uses". Active documentation (Mehrpour, LaToza, Kindi, VL/HCC 2019) shows the rules that apply to the file being edited, their rationale text, and positive and violating examples, which "enables developers to work faster and more successfully" — [LaToza-IEEE](https://people.cs.gmu.edu/~tlatoza/papers/InformationNeeds-LessonsForProgrammingTools.pdf)
- Onboarding barriers include understanding the project structure and complex code, and "outdated, unclear documentation"; they "can take days or more to overcome" (citing Steinmacher et al. 2015 SLR) — [LaToza-IEEE](https://people.cs.gmu.edu/~tlatoza/papers/InformationNeeds-LessonsForProgrammingTools.pdf)
- Design decisions drawn on whiteboards were "subsequently lost" — [WB](https://faculty.washington.edu/ajko/papers/Cherubini2007DevelopersDrawCode.pdf)
- Source code and inline comments are trusted more than documentation (21 participants, against 4 who named documentation as their main source). One said "technical documentation covers only 10% of the application" — [Roehm](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/Roehm2012ICSE.pdf)
- SFM E4 and E11: record hypotheses and *why* a maintainer looks at an object. Discarded hypotheses are useful future documentation. TLES stored "layers of annotations for recording the evolutionary history of source code constructs" — [SFM](https://plg.uwaterloo.ca/~migod/846/papers/storey-jss.pdf)
- Developers put themselves in the end user's role by inspecting UIs to understand behaviour — [Roehm](https://memphis-cs.github.io/comp-7085-8085-2012-fall/papers/Roehm2012ICSE.pdf). This suggests linking code to user-visible features or endpoints.

### Inferences
- Pull rationale from where it is trusted and close to code: Go doc comments (package `doc.go`, type and func comments) shown in place on the map, plus links to the design records (charter/ADR sections, issues, PRs) that introduced or last changed a node. Those links must be bidirectional: from the doc to the nodes it governs, and from a node to the docs and decisions about it.
- Feature or requirement links (endpoint, CLI command, or WP/issue to the code that implements it) answer "what is this supposed to do", which Ko07 found to be deferred often.
- History (the commit or PR that created a node, its message) is a cheap rationale source. I found no study in this pass that measures its effect on comprehension.

### Gaps
- I did not retrieve primary studies that measure the effect of doc comments on comprehension, or of commit-history or requirement-traceability views (e.g., Hipikat, Čubranić and Murphy 2003; Rationalizer, Bradley and Murphy 2011; Aghajani et al. 2019 on documentation issues). These would strengthen this section.
- The claim that active documentation speeds developers up is LaToza's summary. I did not see the effect sizes from the VL/HCC 2019 study.
