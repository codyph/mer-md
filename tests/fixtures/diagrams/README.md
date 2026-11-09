# Diagram coverage corpus

These original examples cover the 31 diagram families listed by the [Mermaid documentation](https://mermaid.js.org/intro/), checked against Mermaid 12.3.0 on 9 November 2026. They exercise representative syntax and visible labels; they do not exhaust every language feature or prove that every possible layout is free of crossings.

The [pinned built-in registration list](https://github.com/mermaid-js/mermaid/blob/mermaid%4012.3.0/packages/mermaid/src/diagram-api/diagram-orchestration.ts) also registers Agentflow, Railroad and Info. The corpus includes them, all four Railroad dialects, all five C4 views, both state headers, the `graph` flowchart alias, and an explicit ELK flowchart. That makes **44 rendering cases**. ZenUML is documented as an external Mermaid integration; the native renderer also implements it and it has a case here.

`catalog.json` records the source reference and visible labels checked for each case. `tests/diagram_families.rs` renders every case through the actual CLI, checks the resulting SVG, then verifies that a second render needs no changes. Internal `error` and malformed front matter input have separate failure tests.

The catalog tracks Mermaid 12.3.0. The native engine reports its own pinned compatibility baseline, 12.1.0, in the Info utility; the fixture checks that version rather than presenting the catalog version as the engine version.

To test one case manually, put its `.mmd` contents inside a fenced `mermaid` block in a Markdown file and run `mer-md` on that file or its containing project. Each input is separate so a failure identifies the affected family.

The SVG canvas and freestanding labels must stay transparent; semantic node fills, chart areas, notes and swimlane bands are part of their respective diagrams. C4's official renderer uses fixed styling and order-based placement, so its reference behavior differs from a general automatic graph layout.
