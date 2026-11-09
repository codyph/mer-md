# Reports and custom colours

This file tests a pie chart and explicit colours that should remain fixed when the surrounding SVG switches between light and dark palettes.

## Example work breakdown

```mermaid
pie title Documentation work
    "Writing" : 45
    "Diagrams" : 30
    "Review" : 25
```

## Explicit styles

```mermaid
flowchart LR
    A[Default theme] --> B[Custom success]
    B --> C[Custom warning]
    C --> D[Default theme]

    classDef success fill:#d1fae5,stroke:#047857,color:#064e3b
    class B success
    style C fill:#fef3c7,stroke:#b45309,color:#78350f
```

The default nodes follow the browser's light/dark theme automatically. The green and amber nodes keep the colours specified in their Mermaid source.
