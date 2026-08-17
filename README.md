# Rustbox

A WIP Bevy 3D course maker / block-builder (Mario Maker–style) with a full Edit/Play loop: place blocks, wire logic, and ship levels. Built on [Repose UI](https://github.com/mlm-games/repose-bevy).

## Features

- **Maker** - place/erase blocks, shapes (ramps, slabs, corners, V-slopes), undo/redo, box fill, paste, mirror, tracks, sign inspector, on/off wiring, online share/download (Cloudflare Worker backend)
- **Edit/Play modes** - edit the level, then playtest in place; win condition, clear screen, checkpoints, death counter and record time
- **Block kit** - terrain, ice, conveyor (incl. on/off + thin), bounce pads, climb, one-way platforms, timed pulse, hang rails
- **Entity kit** - pickups, launch pads, bumpers, gates + keys, fans, prowler, TossCrate, signs, wedges, drift plates, crates, cannons
- **Player** - Bevy + Rapier3d (crates only) + a custom voxel mover: AABB + shaped-surface collision, slopes, step-up, one-ways, hang, ledge grab, wall jump, jump cut / coyote / buffer, drop-through, slam, conveyor/ice, underwater
- **Gamepad + keyboard** - pad play and maker input via Repose
- **Persistence** - RON save/load/export/import with versioned formats
- **i18n** - Fluent-based localization with bundled locales
- **Juice** - squash & stretch, trauma shake, particles, screen effects

## Quick Start

```bash
cargo run
```

Rapier3d is always on (no `physics` feature flag). Dev build with hot-reload:

```bash
cargo run --features dev
```

## Structure

```
src/
├── main.rs              # Entry point
├── app.rs               # AppPlugin, states, system sets
├── maker/               # The course maker
│   ├── editor.rs        # Cursor placement, box fill, paste, mirror, undo
│   ├── level.rs         # LevelDocument: blocks/entities/tracks persistence
│   ├── block.rs         # Block kinds + shapes (rustbox-format)
│   ├── collision.rs     # Custom voxel mover: AABB, shaped surfaces, slopes
│   ├── player.rs        # Player controller: movement, hang, gamepad, juice
│   ├── interaction.rs   # Entities: pads, gates, signs, orbs, crates, respawn
│   ├── entities_runtime.rs # Runtime entity spawn / motion / tracks
│   ├── rapier.rs        # Rapier3d bridge: held crates, seals
│   ├── camera.rs        # Edit orbit + play follow rig
│   ├── rendering.rs     # Chunk meshing, block assets, thumbnails
│   ├── online.rs        # Share / download levels (wasm worker client)
│   ├── commands.rs      # Undoable edit commands
│   └── ...              # ui_bridge, storage, mode, win, campaign, track
├── menus/               # Main, pause, settings, credits (localized)
├── screens/             # Splash, loading, title
└── save.rs              # RON save/load with backup

crates/
├── rustbox-format/      # Level file format, block/entity/track data (shared)
└── rustbox-worker/      # Cloudflare Worker for online levels (wasm32)
```

## Controls (Play)

| Action | Keyboard | Gamepad |
|--------|----------|---------|
| Move | WASD | Left stick |
| Jump / jump cut | Space | South (A / Cross) |
| Crouch / slam / drop-through | Shift / Shift+S | East (B / Circle) |
| Hang | E | West (X / Square) |
| Interact (gates, signs) | I | North (Y / Triangle) |
| Pick up / throw crate | F | Right trigger |
| Reset | R | - |

## Dependencies

| Crate | Purpose |
|-------|---------|
| `bevy` (git rev) | Engine |
| `bevy_rapier3d` | Physics (held crates, dynamic bodies) |
| `rustbox-format` | Level format + worker schema |
| `repose-bevy` / `repose-*` | UI framework |
| `fluent-bundle` + `unic-langid` | Localization (Fluent) |
| `serde` + `ron` + `directories` | Save system |

## License

GPL-3.0