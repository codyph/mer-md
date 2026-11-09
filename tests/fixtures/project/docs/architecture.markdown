# Architecture

These diagrams test multiple diagram types in one `.markdown` file and image links from a nested directory.

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

The text between diagrams should remain in place after rendering and restoring.

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

This closing paragraph should also remain unchanged.
