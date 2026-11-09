# Diagram lifecycle

This state diagram tests a different diagram type and makes a useful target for editing commented source directly.

```mermaid
stateDiagram-v2
    [*] --> Editable
    Editable --> Rendered: render
    Rendered --> Rendered: render unchanged
    Rendered --> Modified: edit commented source
    Editable --> Modified: edit source
    Modified --> Rendered: render updated SVG
    Rendered --> [*]: commit files
```

After rendering this file, change `commit files` to `share documentation` in the source comment. Render again and check that the SVG filename stays the same. The optional restore command can still recover standalone Mermaid fences.
