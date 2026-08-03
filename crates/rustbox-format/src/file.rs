use anyhow::bail;
use serde::{Deserialize, Serialize};

use crate::block::{BlockKind, BlockShape};
use crate::entity::{ContainedItem, EntityData, EntityKind};
use crate::level::{BlockData, BoundaryConfig, ClearCondition, LevelData, LevelTag, Theme};
use crate::track::TrackData;

pub const FORMAT_VERSION: u32 = 6;

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

fn level_format_entities() -> u32 {
    1
}

/// Entity layout used by format versions 2..=5 (before `contents`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityDataV5 {
    pub id: u32,
    pub kind: EntityKind,
    pub cell: [i32; 3],
    #[serde(default)]
    pub yaw_deg: f32,
    #[serde(default = "default_param_v5")]
    pub param: f32,
    #[serde(default)]
    pub cell_b: Option<[i32; 3]>,
    #[serde(default)]
    pub track: Option<crate::track::TrackId>,
    #[serde(default)]
    pub link: u32,
}

fn default_param_v5() -> f32 {
    1.0
}

pub fn upgrade_entity_v5(old: EntityDataV5) -> EntityData {
    EntityData {
        id: old.id,
        kind: old.kind,
        cell: old.cell,
        yaw_deg: old.yaw_deg,
        param: old.param,
        cell_b: old.cell_b,
        track: old.track,
        link: old.link,
        contents: ContainedItem::None,
    }
}

#[derive(Serialize, Deserialize)]
pub struct LevelFile {
    pub version: u32,
    pub level: LevelData,
}

/// Layout of version-4 levels (before clear conditions / checkpoints were added).
#[derive(Serialize, Deserialize)]
pub struct LevelFileV4 {
    pub version: u32,
    pub level: LevelDataV4,
}

#[derive(Serialize, Deserialize)]
pub struct LevelDataV4 {
    pub name: String,
    pub spawn: [i32; 3],
    pub blocks: Vec<BlockData>,
    #[serde(default)]
    pub entities: Vec<EntityDataV5>,
    #[serde(default)]
    pub tracks: Vec<TrackData>,
    #[serde(default = "level_format_entities")]
    pub entities_version: u32,
    #[serde(default)]
    pub author_time: Option<u32>,
    #[serde(default)]
    pub author_deaths: u32,
    #[serde(default)]
    pub record_ms: Option<u32>,
    #[serde(default)]
    pub is_verified: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<LevelTag>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub size: Option<[i32; 3]>,
    #[serde(default)]
    pub water_level: Option<i32>,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub boundary: BoundaryConfig,
    #[serde(default)]
    pub secret_stars: u8,
    #[serde(default)]
    pub coin_star: bool,
}

/// Layout of version-5 levels. `LevelData` gained no fields in v6 — only its
/// entities changed shape (each `EntityData` gained `contents`), so this
/// snapshot differs from `LevelData` purely by its entity type.
#[derive(Serialize, Deserialize)]
pub struct LevelFileV5 {
    pub version: u32,
    pub level: LevelDataV5,
}

#[derive(Serialize, Deserialize)]
pub struct LevelDataV5 {
    pub name: String,
    pub spawn: [i32; 3],
    pub blocks: Vec<BlockData>,
    #[serde(default)]
    pub entities: Vec<EntityDataV5>,
    #[serde(default)]
    pub tracks: Vec<TrackData>,
    #[serde(default = "level_format_entities")]
    pub entities_version: u32,
    #[serde(default)]
    pub author_time: Option<u32>,
    #[serde(default)]
    pub author_deaths: u32,
    #[serde(default)]
    pub record_ms: Option<u32>,
    #[serde(default)]
    pub clear_condition: ClearCondition,
    #[serde(default)]
    pub is_verified: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<LevelTag>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub size: Option<[i32; 3]>,
    #[serde(default)]
    pub water_level: Option<i32>,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub boundary: BoundaryConfig,
    #[serde(default)]
    pub secret_stars: u8,
    #[serde(default)]
    pub coin_star: bool,
}

pub fn upgrade_v5(old: LevelDataV5) -> LevelData {
    LevelData {
        name: old.name,
        spawn: old.spawn,
        blocks: old.blocks,
        entities: old.entities.into_iter().map(upgrade_entity_v5).collect(),
        tracks: old.tracks,
        entities_version: old.entities_version,
        author_time: old.author_time,
        author_deaths: old.author_deaths,
        record_ms: old.record_ms,
        clear_condition: old.clear_condition,
        is_verified: old.is_verified,
        description: old.description,
        tags: old.tags,
        author: old.author,
        created_at: old.created_at,
        size: old.size,
        water_level: old.water_level,
        theme: old.theme,
        boundary: old.boundary,
        secret_stars: old.secret_stars,
        coin_star: old.coin_star,
    }
}

/// Layout of version-3 levels (before times moved to milliseconds).
#[derive(Serialize, Deserialize)]
pub struct LevelFileV3 {
    pub version: u32,
    pub level: LevelDataV3,
}

#[derive(Serialize, Deserialize)]
pub struct LevelDataV3 {
    pub name: String,
    pub spawn: [i32; 3],
    pub blocks: Vec<BlockData>,
    #[serde(default)]
    pub entities: Vec<EntityDataV5>,
    #[serde(default)]
    pub tracks: Vec<TrackData>,
    #[serde(default)]
    pub entities_version: u32,
    #[serde(default)]
    pub author_time: Option<f32>,
    #[serde(default)]
    pub author_deaths: u32,
    #[serde(default)]
    pub record_seconds: Option<f32>,
    #[serde(default)]
    pub is_verified: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<LevelTag>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub size: Option<[i32; 3]>,
    #[serde(default)]
    pub water_level: Option<i32>,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub boundary: BoundaryConfig,
    #[serde(default)]
    pub secret_stars: u8,
    #[serde(default)]
    pub coin_star: bool,
}

#[derive(Serialize, Deserialize)]
pub struct LevelFileV3Pre {
    pub version: u32,
    pub level: LevelDataV3Pre,
}

#[derive(Serialize, Deserialize)]
pub struct LevelDataV3Pre {
    pub name: String,
    pub spawn: [i32; 3],
    pub blocks: Vec<BlockData>,
    #[serde(default)]
    pub entities: Vec<EntityDataV5>,
    #[serde(default)]
    pub tracks: Vec<TrackData>,
    #[serde(default)]
    pub entities_version: u32,
    #[serde(default)]
    pub author_time: Option<f32>,
    #[serde(default)]
    pub author_deaths: u32,
    #[serde(default)]
    pub is_verified: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<LevelTag>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub size: Option<[i32; 3]>,
    #[serde(default)]
    pub water_level: Option<i32>,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub boundary: BoundaryConfig,
    #[serde(default)]
    pub secret_stars: u8,
    #[serde(default)]
    pub coin_star: bool,
}

fn secs_to_ms(t: Option<f32>) -> Option<u32> {
    t.map(|s| (s * 1000.0).round() as u32)
}

pub fn upgrade_v4(old: LevelDataV4) -> LevelData {
    LevelData {
        name: old.name,
        spawn: old.spawn,
        blocks: old.blocks,
        entities: old.entities.into_iter().map(upgrade_entity_v5).collect(),
        tracks: old.tracks,
        entities_version: old.entities_version,
        author_time: old.author_time,
        author_deaths: old.author_deaths,
        record_ms: old.record_ms,
        clear_condition: ClearCondition::ReachGoal,
        is_verified: old.is_verified,
        description: old.description,
        tags: old.tags,
        author: old.author,
        created_at: old.created_at,
        size: old.size,
        water_level: old.water_level,
        theme: old.theme,
        boundary: old.boundary,
        secret_stars: old.secret_stars,
        coin_star: old.coin_star,
    }
}

pub fn upgrade_v3(old: LevelDataV3) -> LevelData {
    LevelData {
        name: old.name,
        spawn: old.spawn,
        blocks: old.blocks,
        entities: old.entities.into_iter().map(upgrade_entity_v5).collect(),
        tracks: old.tracks,
        entities_version: old.entities_version,
        author_time: secs_to_ms(old.author_time),
        author_deaths: old.author_deaths,
        record_ms: secs_to_ms(old.record_seconds),
        clear_condition: ClearCondition::ReachGoal,
        is_verified: old.is_verified,
        description: old.description,
        tags: old.tags,
        author: old.author,
        created_at: old.created_at,
        size: old.size,
        water_level: old.water_level,
        theme: old.theme,
        boundary: old.boundary,
        secret_stars: old.secret_stars,
        coin_star: old.coin_star,
    }
}

pub fn upgrade_v3pre(old: LevelDataV3Pre) -> LevelData {
    LevelData {
        name: old.name,
        spawn: old.spawn,
        blocks: old.blocks,
        entities: old.entities.into_iter().map(upgrade_entity_v5).collect(),
        tracks: old.tracks,
        entities_version: old.entities_version,
        author_time: secs_to_ms(old.author_time),
        author_deaths: old.author_deaths,
        record_ms: None,
        clear_condition: ClearCondition::ReachGoal,
        is_verified: old.is_verified,
        description: old.description,
        tags: old.tags,
        author: old.author,
        created_at: old.created_at,
        size: old.size,
        water_level: old.water_level,
        theme: old.theme,
        boundary: old.boundary,
        secret_stars: old.secret_stars,
        coin_star: old.coin_star,
    }
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
    entities: Vec<EntityDataV5>,
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
    entities: Vec<EntityDataV5>,
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
        entities: old.entities.into_iter().map(upgrade_entity_v5).collect(),
        tracks: old.tracks,
        entities_version: old.entities_version,
        author_time: secs_to_ms(old.author_time),
        author_deaths: old.author_deaths,
        record_ms: None,
        clear_condition: ClearCondition::ReachGoal,
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
    let version = {
        let head: [u8; 4] = bytes
            .get(..4)
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| anyhow::anyhow!("corrupted level"))?;
        u32::from_le_bytes(head)
    };
    match version {
        6 => Ok(bincode::deserialize::<LevelFile>(&bytes)?.level),
        5 => Ok(upgrade_v5(
            bincode::deserialize::<LevelFileV5>(&bytes)?.level,
        )),
        4 => Ok(upgrade_v4(
            bincode::deserialize::<LevelFileV4>(&bytes)?.level,
        )),
        3 => {
            // v3 levels exist in two layouts (before/after record_seconds).
            if let Ok(old) = bincode::deserialize::<LevelFileV3>(&bytes) {
                Ok(upgrade_v3(old.level))
            } else {
                let old: LevelFileV3Pre =
                    bincode::deserialize(&bytes).map_err(|_| anyhow::anyhow!("corrupted level"))?;
                Ok(upgrade_v3pre(old.level))
            }
        }
        2 => {
            let old: LevelFileV2 =
                bincode::deserialize(&bytes).map_err(|_| anyhow::anyhow!("corrupted level"))?;
            Ok(upgrade_v2(old.level))
        }
        1 => {
            let old: LevelFileV1 =
                bincode::deserialize(&bytes).map_err(|_| anyhow::anyhow!("corrupted level"))?;
            Ok(upgrade_v1(old.level))
        }
        v => bail!("unknown level format version {v}"),
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
        if e.contents != ContainedItem::None {
            if !e.kind.supports_contents() {
                bail!("{} cannot contain items", e.kind.label());
            }
            if e.kind == EntityKind::Crate && e.param < 0.5 {
                bail!("an unbreakable crate contains an item that can never be reached");
            }
            if let ContainedItem::Glimmers(n) = e.contents
                && n == 0
            {
                bail!("a container holds zero glimmers");
            }
            if e.contents == ContainedItem::Key && !(1..=9).contains(&e.link) {
                bail!("a contained key needs its container on link channel 1-9");
            }
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
    if let Some(w) = level.water_level {
        if w.abs() > MAX_COORD {
            bail!("water level out of bounds");
        }
    }
    if let ClearCondition::TimeLimitMs(ms) = level.clear_condition
        && ms == 0
    {
        bail!("time limit must be greater than zero");
    }

    // Link sanity: teleporters should come in pairs per channel; lock gates
    // need at least one matching key on the same channel.
    let mut tele_links = [0u32; 10];
    let mut key_links = [0u32; 10];
    let mut lock_links = [0u32; 10];

    for e in &level.entities {
        match e.kind {
            crate::entity::EntityKind::Teleporter if e.link >= 1 && e.link <= 9 => {
                tele_links[e.link as usize] += 1
            }
            crate::entity::EntityKind::Key if e.link >= 1 && e.link <= 9 => {
                key_links[e.link as usize] += 1
            }
            crate::entity::EntityKind::LockGate if e.link >= 1 && e.link <= 9 => {
                lock_links[e.link as usize] += 1
            }
            _ => {}
        }

        // Contained keys satisfy lock gates on the container's channel.
        if e.contents == ContainedItem::Key && (1..=9).contains(&e.link) {
            key_links[e.link as usize] += 1;
        }
    }

    for ch in 1..=9 {
        if tele_links[ch] == 1 {
            bail!("teleporter link {ch} needs a pair");
        }
        if lock_links[ch] > 0 && key_links[ch] == 0 {
            bail!("lock gate on channel {ch} has no key");
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
    use crate::entity::{ContainedItem, EntityData, EntityKind};
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
                contents: ContainedItem::None,
            }],
            tracks: vec![],
            entities_version: 1,
            author_time: None,
            author_deaths: 0,
            record_ms: None,
            clear_condition: ClearCondition::ReachGoal,
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

    #[test]
    fn upgrade_v4_defaults_clear_condition() {
        let old = LevelDataV4 {
            name: "Old".into(),
            spawn: [0, 1, 0],
            blocks: vec![],
            entities: vec![],
            tracks: vec![],
            entities_version: 1,
            author_time: None,
            author_deaths: 0,
            record_ms: None,
            is_verified: false,
            description: String::new(),
            tags: vec![],
            author: String::new(),
            created_at: 0,
            size: None,
            water_level: None,
            theme: Theme::Grass,
            boundary: BoundaryConfig::default(),
            secret_stars: 0,
            coin_star: false,
        };

        let upgraded = upgrade_v4(old);
        assert_eq!(upgraded.clear_condition, ClearCondition::ReachGoal);
    }

    #[test]
    fn v6_contents_roundtrip() {
        let mut level = sample_level();
        level.entities.push(EntityData {
            id: 99,
            kind: EntityKind::Crate,
            cell: [1, 1, 1],
            yaw_deg: 0.0,
            param: 1.0,
            cell_b: None,
            track: None,
            link: 3,
            contents: ContainedItem::Key,
        });
        let bytes = encode_level(&level).unwrap();
        let back = decode_level(&bytes).unwrap();
        assert_eq!(back.entities.last().unwrap().contents, ContainedItem::Key);
        assert_eq!(back.entities.last().unwrap().link, 3);
    }

    #[test]
    fn upgrade_v5_defaults_contents_to_none() {
        let old = EntityDataV5 {
            id: 1,
            kind: EntityKind::Crate,
            cell: [0, 0, 0],
            yaw_deg: 0.0,
            param: 1.0,
            cell_b: None,
            track: None,
            link: 0,
        };
        assert_eq!(upgrade_entity_v5(old).contents, ContainedItem::None);
    }

    fn crate_with(contents: ContainedItem) -> EntityData {
        EntityData {
            id: 7,
            kind: EntityKind::Crate,
            cell: [1, 1, 1],
            yaw_deg: 0.0,
            param: 1.0,
            cell_b: None,
            track: None,
            link: 1,
            contents,
        }
    }

    #[test]
    fn validate_rejects_bad_contents() {
        let mut glimmer_in_key = sample_level();
        glimmer_in_key.entities.push(EntityData {
            id: 7,
            kind: EntityKind::Glimmer,
            cell: [1, 1, 1],
            yaw_deg: 0.0,
            param: 1.0,
            cell_b: None,
            track: None,
            link: 0,
            contents: ContainedItem::Key,
        });
        assert!(validate_level(&glimmer_in_key).is_err());

        let mut unbreakable = sample_level();
        unbreakable
            .entities
            .push(crate_with(ContainedItem::HealOrb));
        if let Some(e) = unbreakable.entities.last_mut() {
            e.param = 0.0;
        }
        assert!(validate_level(&unbreakable).is_err());

        let mut zero = sample_level();
        zero.entities.push(crate_with(ContainedItem::Glimmers(0)));
        assert!(validate_level(&zero).is_err());

        let mut unlinked_key = sample_level();
        unlinked_key.entities.push(crate_with(ContainedItem::Key));
        if let Some(e) = unlinked_key.entities.last_mut() {
            e.link = 0;
        }
        assert!(validate_level(&unlinked_key).is_err());

        // A contained key counts toward the lock gate's key requirement.
        let mut crate_key = sample_level();
        crate_key.entities.push(crate_with(ContainedItem::Key));
        crate_key.entities.push(EntityData {
            id: 8,
            kind: EntityKind::LockGate,
            cell: [2, 1, 1],
            yaw_deg: 0.0,
            param: 0.0,
            cell_b: None,
            track: None,
            link: 1,
            contents: ContainedItem::None,
        });
        assert!(validate_level(&crate_key).is_ok());
    }
}
