# Crates

Each crate uses only the crates below it.

```
        arbiter
        ┌──┴──┐
        │    app
        │   ┌─┴──┐
      project   gui
     ┌──┴──┐
   pack   game
     └──┬──┘
     formats
        │
      diag
```

- **`arbiter`**: the CLI. It parses arguments, calls `project`, and prints. With no command, it calls `app::run`.
- **`project`**: everything the user does: open, edit, undo, save, check, build; and project management.
- **`pack`**: unpacking and building (packing) to a target. New build targets go here.
- **`game`**: definitions for raw game values, editions, and table-specific checks. A game without tables still works, just with raw values.
- **`formats`**: bytes to structs and back. Game-data agnostic.
- **`diag`**: the diagnostic types and the sink.
- **`app`**: the editor. Everything the user sees, drawn through `gui`.
- **`gui`**: windowing, input, layout, and drawing. Knows nothing of Arbiter.

The workspace denies clippy `pedantic` and every way to panic. `formats` also
bans native-endian byte conversions in its own `clippy.toml`.
