# Crates

| Crate | What it does |
|---|---|
| `arbiter` | Entry-point CLI. With no command, runs `app`. |
| `app` | The editor interface. |
| `gui` | The gui engine: windowing, input, layout, drawing. |
| `project` | Project storage: file tree, reading and saving, check, build, project management. |
| `pack` | Unpacking a disc, and building one back. New build targets go here. |
| `formats` | Bytes to structs and back, and editing them. |
| `diag` | Diagnostic utility. |

## Editing a file

`app` takes each step through one crate:

- **Open** with `project`: the file, with its changes from `changes/` applied when opened that way.
- **Read** with `formats`: the format's structs.
- **Edit** with `formats`: the format's `Edit`, which returns the edit that undoes it.
- **Save** with `project`: a diff in `changes/`, or the bytes.

`pack` takes no part in editing, only in setting up `base/` and building.

## Where a format lives

Each format sits in `formats/src/<library>/<module>/<format>/`, such as `jsystem/jmessage/bmg/`:
bytes, structs and edits, game agnostic.
