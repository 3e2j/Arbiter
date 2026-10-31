<div align="center">

# Arbiter

</div>

Arbiter is an editor for viewing, editing, and building mods for Twilight Princess.

Other games built on the same libraries can be browsed too, or forked to support. Only Twilight Princess is maintained here.

> [!IMPORTANT]
> Arbiter is in early development. Don't expect anything to work properly yet.

---

## Why?
Existing tools for Nintendo's games are few and far between, and most were built for a single use-case.
Because of this, many features are missing and compatibility is not guaranteed.

Format converters are also flawed because they treat game files in isolation. They don't understand
game-specifics or interact with the game itself.

Arbiter's goal is to unify these tools under one roof, and to understand how formats fit together as a whole.

## Features
- Unpack game content
- Inspect, edit, and create assets for the game
- Build and package mods
- Launch builds to test them (in emulators like [Dolphin](https://dolphin-emu.org/) or [Dusklight](https://twilitrealm.dev/))

## History
Arbiter is a ground-up rebuild of [TPMT (Twilight Princess Modding Toolkit)](https://github.com/3e2j/TPMT).
See its README for the project's history and why it was restarted.

## Credits
Arbiter's interface is heavily inspired by [Godot](https://godotengine.org/) and [Zed](https://zed.dev/)
whose approachable, user-first design shaped much of how Arbiter looks and works.

Twilight Princess definitions and game structures come from the
[Twilight Princess decompilation](https://github.com/zeldaret/tp)
and [Dusklight](https://github.com/TwilitRealm/dusklight).
