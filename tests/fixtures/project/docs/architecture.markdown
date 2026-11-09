# Architecture

The documentation model connects projects, documents, and diagrams.

## Classes

```mermaid
classDiagram
    Project "1" --> "many" Document
    Document "1" --> "many" Diagram

    class Project {
        name
        rootDirectory
    }
    class Document {
        path
        render()
        restore()
    }
    class Diagram {
        id
        source
        svgPath
    }
```

Documents contain diagram sources and reference their generated images.

## Relationships

```mermaid
erDiagram
    PROJECT ||--o{ DOCUMENT : contains
    DOCUMENT ||--o{ DIAGRAM : defines
    DOCUMENT {
        string path
    }
    DIAGRAM {
        string id
        string source
        string svgPath
    }
```

Diagram identities persist across source edits.
