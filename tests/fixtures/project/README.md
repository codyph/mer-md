# Documentation pipeline

Mermaid diagrams describe the documentation pipeline and editing workflow.

## Rendering workflow

```mermaid
flowchart LR
    A[Read Markdown] --> B{Mermaid blocks found?}
    B -->|Yes| C[Render SVG images]
    C --> D[Update image references]
    B -->|No| E[Keep file unchanged]
```

Rendering replaces diagram sources with references to generated SVG images.

## Editing workflow

Diagram identities persist across source edits.

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

See [architecture](docs/architecture.markdown), [workflows](docs/workflows.md), and [reports](docs/reports.markdown).
