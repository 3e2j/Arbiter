# Crates

A crate uses only the crates listed below it, so no two ever loop.

| Crate | What it does |
|---|---|
| `arbiter` | The CLI. Parses arguments, calls `project`, prints. With no command, runs `app`. |
| `app` | The editor. Everything the user sees, drawn through `gui`. |
| `gui` | The engine: windowing, input, layout, drawing. Knows nothing of Arbiter. |
| `project` | Storage: how files sit on disk, reading and saving them, check, build, project management. |
| `pack` | Unpacking a disc into `base/`, and building one back. New build targets go here. |
| `formats` | Bytes to structs and back, editing them, and what each game reads the raw values as. |
| `diag` | The diagnostic types and the sink. |

## Editing a file

`app` takes each step through one crate:

- **Open** with `project`: the file, with its changes from `changes/` applied when opened that way.
- **Read** with `formats`: the format's structs, and the game's tables.
- **Edit** with `formats`: the format's `Edit`, which returns the edit that undoes it.
- **Save** with `project`: a diff in `changes/`, or the bytes.

`pack` takes no part in editing, only in setting up `base/` and building.

## Where a format lives

Each format sits in `formats/src/<library>/<module>/`, such as `jsystem/jmessage/`:

- `<format>/`: bytes, structs and edits. Knows no game.
- `games/`: what each game reads the values as. Uses the formats, never the reverse.
