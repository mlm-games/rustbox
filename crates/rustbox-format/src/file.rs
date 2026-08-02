use anyhow::bail;
use serde::{Deserialize, Serialize};

use crate::block::{BlockKind, BlockShape};
use crate::entity::EntityData;
use crate::level::{BlockData, LevelData, Theme};
use crate::track::TrackData;

pub const FORMAT_VERSION: u32 = 3;

/// Upper bound for inflating untrusted levels (tiny compressed input must not
/// explode into gigabytes of memory).
pub const MAX_DECOMPRESSED_LEVEL_SIZE: usize = 8 * 1024 * 1024;

pub const MAX_NAME_LEN: usize = 64;
pub const MAX_AUTHOR_LEN: usize = 64;
pub const MAX_DESCRIPTION_LEN: usize = 1_024;
pub const MAX_BLOCKS: usize = 100_000;
pub const MAX_ENTITIES: usize = 2_000;
pub const MAX_TRACKS: usize = 64;
pub const MAX_TRACK_POINTS: usize = 512;
pub const MAX_COORD: i32 = 512;

#[derive(Serialize, Deserialize)]
pub struct LevelFile {
    pub version: u32,
    pub level: LevelData,
}

/// Layout of version-2 levels (before size/water/theme were added). Only
/// needed to decode old share codes, which use bincode (binary, so serde
/// defaults cannot fill in missing trailing fields).
#[derive(Serialize, Deserialize)]
struct LevelFileV2 {
    version: u32,
    level: LevelDataV2,
}

/// Blocks as they were serialized before shape/rot/waterlogged existed.
#[derive(Serialize, Deserialize)]
struct BlockDataV1 {
    position: [i32; 3],
    kind: BlockKind,
}

impl BlockDataV1 {
    fn upgrade(self) -> BlockData {
        BlockData {
            position: self.position,
            kind: self.kind,
            shape: BlockShape::Full,
            rot: 0,
            waterlogged: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct LevelDataV2 {
    name: String,
    spawn: [i32; 3],
    blocks: Vec<BlockDataV1>,
    #[serde(default)]
    entities: Vec<EntityData>,
    #[serde(default)]
    tracks: Vec<TrackData>,
    #[serde(default)]
    entities_version: u32,
    #[serde(default)]
    author_time: Option<f32>,
    #[serde(default)]
    author_deaths: u32,
    #[serde(default)]
    is_verified: bool,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<crate::level::LevelTag>,
    #[serde(default)]
    author: String,
    #[serde(default)]
    created_at: u64,
}

/// Layout of version-1 levels (before description/tags/author/created_at were
/// added).
#[derive(Serialize, Deserialize)]
struct LevelFileV1 {
    version: u32,
    level: LevelDataV1,
}

#[derive(Serialize, Deserialize)]
struct LevelDataV1 {
    name: String,
    spawn: [i32; 3],
    blocks: Vec<BlockDataV1>,
    #[serde(default)]
    entities: Vec<EntityData>,
    #[serde(default)]
    tracks: Vec<TrackData>,
    #[serde(default)]
    entities_version: u32,
    #[serde(default)]
    author_time: Option<f32>,
    #[serde(default)]
    author_deaths: u32,
    #[serde(default)]
    is_verified: bool,
}

fn upgrade_v2(old: LevelDataV2) -> LevelData {
    LevelData {
        name: old.name,
        spawn: old.spawn,
        blocks: old.blocks.into_iter().map(BlockDataV1::upgrade).collect(),
        entities: old.entities,
        tracks: old.tracks,
        entities_version: old.entities_version,
        author_time: old.author_time,
        author_deaths: old.author_deaths,
        is_verified: old.is_verified,
        description: old.description,
        tags: old.tags,
        author: old.author,
        created_at: old.created_at,
        size: None,
        water_level: None,
        theme: Theme::Grass,
        boundary: Default::default(),
        secret_stars: 0,
        coin_star: false,
    }
}

fn upgrade_v1(old: LevelDataV1) -> LevelData {
    upgrade_v2(LevelDataV2 {
        name: old.name,
        spawn: old.spawn,
        blocks: old.blocks,
        entities: old.entities,
        tracks: old.tracks,
        entities_version: old.entities_version,
        author_time: old.author_time,
        author_deaths: old.author_deaths,
        is_verified: old.is_verified,
        description: String::new(),
        tags: vec![],
        author: String::new(),
        created_at: 0,
    })
}

/// Serialize + DEFLATE a level into the compact wire format (raw bytes, no
/// base64). Shared by the game (uploads) and the Worker (downloads).
pub fn encode_level(level: &LevelData) -> anyhow::Result<Vec<u8>> {
    let file = LevelFile {
        version: FORMAT_VERSION,
        level: level.clone(),
    };
    let bytes = bincode::serialize(&file)?;
    Ok(miniz_oxide::deflate::compress_to_vec(&bytes, 6))
}

/// Decompress + deserialize a level from the wire format, bounded against
/// decompression bombs.
pub fn decode_level(compressed: &[u8]) -> anyhow::Result<LevelData> {
    let bytes =
        miniz_oxide::inflate::decompress_to_vec_with_limit(compressed, MAX_DECOMPRESSED_LEVEL_SIZE)
            .map_err(|_| anyhow::anyhow!("corrupted level (inflate failed)"))?;
    match bincode::deserialize::<LevelFile>(&bytes) {
        Ok(file) => match file.version {
            1 | 2 | 3 => Ok(file.level),
            v => bail!("unknown level format version {v}"),
        },
        Err(_) => match bincode::deserialize::<LevelFileV2>(&bytes) {
            Ok(old) => {
                if old.version != 2 {
                    bail!("unknown level format version {}", old.version);
                }
                Ok(upgrade_v2(old.level))
            }
            Err(_) => {
                // Oldest codes were produced with an even smaller struct.
                let old: LevelFileV1 =
                    bincode::deserialize(&bytes).map_err(|_| anyhow::anyhow!("corrupted level"))?;
                if old.version != 1 {
                    bail!("unknown level format version {}", old.version);
                }
                Ok(upgrade_v1(old.level))
            }
        },
    }
}

/// Copyable share-code string: base64(URL-safe, no padding) of `encode_level`.
pub fn export_code(level: &LevelData) -> anyhow::Result<String> {
    use base64::Engine as _;
    let bytes = encode_level(level)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// Inverse of `export_code`.
pub fn import_code(code: &str) -> anyhow::Result<LevelData> {
    use base64::Engine as _;
    let compressed = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(code)?;
    decode_level(&compressed)
}

/// Structural validation for untrusted level data (uploads/downloads). Checks
/// counts, coordinate bounds, string lengths, and finite floats. Serialization
/// already guarantees valid enum variants, so no kind checks are needed here.
pub fn validate_level(level: &LevelData) -> anyhow::Result<()> {
    if level.name.chars().count() > MAX_NAME_LEN {
        bail!("level name too long");
    }
    if level.author.chars().count() > MAX_AUTHOR_LEN {
        bail!("level author too long");
    }
    if level.description.chars().count() > MAX_DESCRIPTION_LEN {
        bail!("level description too long");
    }
    if level.blocks.len() > MAX_BLOCKS {
        bail!("too many blocks");
    }
    if level.entities.len() > MAX_ENTITIES {
        bail!("too many entities");
    }
    if level.tracks.len() > MAX_TRACKS {
        bail!("too many tracks");
    }
    if !in_bounds(&level.spawn) {
        bail!("spawn out of bounds");
    }
    for b in &level.blocks {
        if !in_bounds(&b.position) {
            bail!("block out of bounds");
        }
    }
    for e in &level.entities {
        if !in_bounds(&e.cell) {
            bail!("entity out of bounds");
        }
        if !e.yaw_deg.is_finite() || !e.param.is_finite() {
            bail!("entity has a non-finite value");
        }
    }
    for t in &level.tracks {
        if t.points.len() > MAX_TRACK_POINTS {
            bail!("track has too many points");
        }
        if !t.speed.is_finite() {
            bail!("track has a non-finite speed");
        }
        for p in &t.points {
            if !in_bounds(p) {
                bail!("track point out of bounds");
            }
        }
    }
    if level.author_time.is_some_and(|t| !t.is_finite()) {
        bail!("author time is not finite");
    }
    if let Some(w) = level.water_level {
        if w.abs() > MAX_COORD {
            bail!("water level out of bounds");
        }
    }
    Ok(())
}

fn in_bounds(cell: &[i32; 3]) -> bool {
    cell.iter().all(|c| c.abs() <= MAX_COORD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockKind, BlockShape};
    use crate::entity::{EntityData, EntityKind};
    use crate::level::BlockData;

    fn sample_level() -> LevelData {
        LevelData {
            name: "Test".into(),
            spawn: [0, 1, 0],
            blocks: vec![BlockData {
                position: [0, 0, 0],
                kind: BlockKind::Grass,
                shape: BlockShape::Half,
                rot: 1,
                waterlogged: false,
            }],
            entities: vec![EntityData {
                id: 1,
                kind: EntityKind::Glimmer,
                cell: [0, 1, 0],
                yaw_deg: 0.0,
                param: 1.0,
                cell_b: None,
                track: None,
                link: 0,
            }],
            tracks: vec![],
            entities_version: 1,
            author_time: None,
            author_deaths: 0,
            is_verified: true,
            description: String::new(),
            tags: vec![],
            author: String::new(),
            created_at: 0,
            size: None,
            water_level: Some(2),
            theme: Theme::Cave,
            boundary: Default::default(),
            secret_stars: 3,
            coin_star: true,
        }
    }

    #[test]
    fn encode_decode_round_trips() {
        let lvl = sample_level();
        let bytes = encode_level(&lvl).unwrap();
        let back = decode_level(&bytes).unwrap();
        assert_eq!(lvl.name, back.name);
        assert_eq!(lvl.blocks.len(), back.blocks.len());
        assert_eq!(lvl.entities.len(), back.entities.len());
        assert_eq!(lvl.theme, back.theme);
        assert_eq!(lvl.water_level, back.water_level);
        assert_eq!(lvl.secret_stars, back.secret_stars);
        assert_eq!(lvl.blocks, back.blocks);
    }

    #[test]
    fn export_import_code_round_trips() {
        let lvl = sample_level();
        let code = export_code(&lvl).unwrap();
        let back = import_code(&code).unwrap();
        assert_eq!(lvl.blocks, back.blocks);
        assert_eq!(lvl.water_level, back.water_level);
    }

    #[test]
    fn validate_rejects_bad_levels() {
        let good = sample_level();
        assert!(validate_level(&good).is_ok());

        let mut long_name = good.clone();
        long_name.name = "x".repeat(MAX_NAME_LEN + 1);
        assert!(validate_level(&long_name).is_err());

        let mut oob = good.clone();
        oob.spawn = [MAX_COORD + 1, 0, 0];
        assert!(validate_level(&oob).is_err());

        let mut nan = good.clone();
        nan.entities[0].param = f32::NAN;
        assert!(validate_level(&nan).is_err());
    }

    #[test]
    fn bounded_decompression_rejects_bomb() {
        // A tiny deflate stream that claims to expand far past the limit.
        let bomb =
            miniz_oxide::deflate::compress_to_vec(&vec![0u8; MAX_DECOMPRESSED_LEVEL_SIZE + 1], 6);
        assert!(decode_level(&bomb).is_err());
    }

    #[test]
    fn v1_code_upgrades_without_shape_rot() {
        // Old codes serialized blocks as {position, kind} only.
        let old = LevelFileV1 {
            version: 1,
            level: LevelDataV1 {
                name: "Legacy".into(),
                spawn: [0, 1, 0],
                blocks: vec![BlockDataV1 {
                    position: [1, 2, 3],
                    kind: BlockKind::Stone,
                }],
                entities: vec![],
                tracks: vec![],
                entities_version: 1,
                author_time: None,
                author_deaths: 0,
                is_verified: false,
            },
        };
        let bytes = bincode::serialize(&old).unwrap();
        let compressed = miniz_oxide::deflate::compress_to_vec(&bytes, 6);
        let lvl = decode_level(&compressed).unwrap();
        assert_eq!(lvl.blocks.len(), 1);
        assert_eq!(lvl.blocks[0].position, [1, 2, 3]);
        assert_eq!(lvl.blocks[0].kind, BlockKind::Stone);
        assert_eq!(lvl.blocks[0].shape, BlockShape::Full);
        assert_eq!(lvl.blocks[0].rot, 0);
        assert!(!lvl.blocks[0].waterlogged);
        assert_eq!(lvl.theme, Theme::Grass);
        assert_eq!(lvl.water_level, None);
    }
}
