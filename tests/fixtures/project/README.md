# Sample documentation project

This file contains two independent diagrams with ordinary Markdown between them. Rendering should keep each diagram in its original position and preserve the surrounding text.

## Rendering workflow

After rendering, change `Read Markdown` to `Read project files` directly in the commented source. Rendering again should update the same SVG.

```mermaid
flowchart LR
    A[Read Markdown] --> B{Mermaid blocks found?}
    B -->|Yes| C[Render SVG images]
    C --> D[Update image references]
    B -->|No| E[Keep file unchanged]
```

This paragraph should remain between the two diagrams after rendering and restoring.

## Editing workflow

This second diagram tests participant labels, messages, a note, and more than one diagram in a single file.

```mermaid
sequenceDiagram
    participant U as User
    participant T as Mermaid tool
    participant F as Project files
    U->>F: Edit the commented Mermaid source
    Note over U,T: Diagram IDs stay unchanged
    U->>T: Render Markdown
    T->>F: Replace the existing SVG
    T-->>U: Ready to commit
```

See [architecture](docs/architecture.markdown), [workflows](docs/workflows.md), and [reports](docs/reports.markdown) for nested Markdown files.
