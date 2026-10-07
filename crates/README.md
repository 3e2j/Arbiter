# Crates

Each crate uses only the crates below it.

```
   arbiter
      │
   project
   ┌──┴──┐
 pack   game
   └──┬──┘
   formats
      │
    diag
```

- **`arbiter`**: the CLI. It parses arguments, calls `project`, and prints.
- **`project`**: everything the user does: open, edit, undo, save, check, build; and project management.
- **`pack`**: unpacking and building (packing) to a target. New build targets go here.
- **`game`**: definitions for raw game values, and the checks that need them. A game without tables still works, just with raw values.
- **`formats`**: bytes to structs and back. Game-data agnostic.
- **`diag`**: the diagnostic types and the sink.

The workspace denies clippy `pedantic` and every way to panic. `formats` also
bans native-endian byte conversions in its own `clippy.toml`.
