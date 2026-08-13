# 3D Models

All in-game models come from **Quaternius' Cube World Kit** under the
**CC0 1.0 Universal** license
(<https://creativecommons.org/publicdomain/zero/1.0/>).

sources:

- Quaternius site: <https://quaternius.com/packs/cubeworldkit.html>
- poly.pizza: <https://poly.pizza/bundle/Cube-World-Kit-DwDr8493Fw>

## Models used

| File                          | Used for       |
| ----------------------------- | -------------- |
| `cubeworld/Character_Male_2.gltf` | Player (animated character) |
| `cubeworld/Goblin.gltf`       | Prowler (animated enemy, clip source) |
| `rustbox/blocks/*.glb`        | Block kinds × shapes (170 models) |
| `rustbox/entities/*.glb`      | Entity kitbashes (22)   |

Models are self-contained glTF 2.0 (embedded buffers).

## Runtime wiring

The active visuals come from `assets/models/rustbox/`, authored from the pack's
CC0 primitives (via `tools/asset_build/`) so every block shape and entity
shares the Cube World visual language. `assets/models/{blocks,entities}.ron`
map each kind/shape to its glTF model; the `cubeworld/*.gltf` files remain as
the built-in fallback and animation-clip source.
