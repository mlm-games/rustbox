# Rustbox

A WIP 3D course maker / block-builder (MMaker–style) with a full Edit/Play loop: place blocks, wire logic, and ship levels. Built on [Repame](https://github.com/mlm-games/repame) and [Repose UI](https://github.com/mlm-games/repose-bevy).

## Features

- **Maker** - place/erase blocks, shapes (ramps, slabs, corners, V-slopes), undo/redo, box fill, paste, mirror, tracks, sign inspector, on/off wiring, online share/download (Cloudflare Worker backend)
- **Edit/Play modes** - edit the level, then playtest in place; win condition, clear screen, checkpoints, death counter and record time
- **Block kit** - terrain, ice, conveyor (incl. on/off + thin), bounce pads, climb, one-way platforms, timed pulse, hang rails
- **Entity kit** - pickups, launch pads, bumpers, gates + keys, fans, prowler, TossCrate, signs, wedges, drift plates, crates, cannons
- **Player** - Rapier3d (crates only) + a custom voxel mover: AABB + shaped-surface collision, slopes, step-up, one-ways, hang, jump cut / coyote / buffer, drop-through, slam, conveyor/ice, underwater. Ledge grab and wall kick exist as tuning flags but are **off by default** (jank-heavy)
- **Gamepad + keyboard** - pad play (left stick, buttons, right-stick camera) and maker input via Repose
- **Timed pulse blocks** - free-running solid/empty clock in Play, independent of the On/Off switch channel
- **Persistence** - RON save/load/export/import with versioned formats
- **i18n** - Fluent-based localization with bundled locales
- **Models** - glTF asset pack (`assets/models/*.ron`) drives blocks and entities; the player is a skinned model with idle/run/jump clips
- **Juice** - squash & stretch, trauma shake, white flash

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
├── lib.rs               # App: sim wiring, input, frame assembly, menus
├── maker/               # The course maker
│   ├── level.rs         # LevelDocument: blocks/entities/tracks persistence
│   ├── block.rs         # Block kinds + shapes (rustbox-format)
│   ├── collision.rs     # Custom voxel mover: AABB, shaped surfaces, slopes
│   ├── player.rs        # Player controller: movement, hang, gamepad, squash
│   ├── interaction.rs   # Entities: pads, gates, signs, orbs, crates, respawn
│   ├── interactive_blocks.rs # On/Off channel + timed pulse clock
│   ├── entities_runtime.rs # Runtime entity spawn / motion / tracks / drawing
│   ├── rapier.rs        # Rapier3d bridge: held crates, seals
│   ├── camera.rs        # Edit orbit + play follow rig (free-look, right stick)
│   ├── level_view.rs    # Chunk meshing, edit tools, per-cell model instances
│   ├── assets/          # glTF import: pack bytes (generated) + asset manifests
│   ├── characters.rs    # Skinned player model, clip selection, facing
│   ├── backdrop.rs      # Water plane + boundary box
│   ├── theme.rs         # Per-level sky, ambient, water tint
│   ├── screen.rs        # Trauma shake, white flash
│   ├── gizmos.rs        # Edit-mode overlays: selection, paste, box fill, tracks
│   ├── edit_ops.rs      # Cursor placement, box fill, paste, mirror, undo
│   ├── commands.rs      # Undoable edit commands
│   ├── online.rs        # Share / download levels (wasm worker client)
│   └── ...              # storage, mode, win, campaign, track, chunk, limits,
│                        # catalog, thumbnail, palette, creator, props
├── menus/               # Main, pause, settings, credits (localized)
└── save.rs              # RON save/load with backup

crates/
├── rustbox-format/      # Level file format, block/entity/track data (shared)
└── rustbox-worker/      # Cloudflare Worker for online levels (wasm32)
```

Regenerate the embedded model table after changing an asset manifest:

```bash
python3 tools/asset_build/gen_asset_pack.py
```

## Controls (Play)

| Action | Keyboard | Gamepad |
|--------|----------|---------|
| Move | WASD | Left stick |
| Look (camera) | Mouse (cursor locked) | Right stick |
| Jump / jump cut | Space | South (A / Cross) |
| Crouch / slam / drop-through | Shift / Shift+S | East (B / Circle) |
| Hang | E | West (X / Square) |
| Interact (gates, signs) | I | North (Y / Triangle) |
| Pick up / throw crate | F | Right trigger |
| Reset | R | Select |

## Dependencies

| Crate | Purpose |
|-------|---------|
| `repame-shell` / `repame-sim` | Shell + fixed-step simulation |
| `repame-view3d` | Voxel meshing, glTF/skin import, batching, lighting |
| `repame-rapier3d` | Physics (held crates, dynamic bodies) |
| `repose-core` / `repose-ui` | UI framework |
| `rustbox-format` | Level format + worker schema |
| `game-utils` | i18n (Fluent) + save manager |
| `serde` + `ron` + `directories` | Save system |

## License

GPL-3.0
