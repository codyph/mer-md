# mer-md

Turn Mermaid in Markdown into SVGs. Portable, editable, and fully offline.

Render a project or file:

```powershell
.\mer-md.exe .
.\mer-md.exe README.md
```

On Linux and macOS:

```sh
chmod +x mer-md
./mer-md .
```

SVGs are saved under the selected directory's `docs/diagrams/`. Source remains editable in adjacent HTML comments; run again after editing to update the same SVGs.

Restore Mermaid fences with `mer-md . --restore`. Use `mer-md --help` for usage.

See `demo/README.md` for source and generated SVGs. Images are centred in viewers supporting HTML, and colours follow the browser's light or dark preference.

Dependency licences are included in `THIRD_PARTY_LICENSES.txt`.
