use bevy::prelude::*;

use super::level::LevelData;
use super::storage::deserialize_level;

pub struct BundledLevel {
    pub id: &'static str,
    pub name: &'static str,
    /// Which verb this level teaches (shown as subtitle)
    pub teaches: &'static str,
    pub source: &'static str,
}

/// Embedded at compile time — works identically on native and web.
pub const BUNDLED_LEVELS: &[BundledLevel] = &[
    BundledLevel {
        id: "01_first_steps",
        name: "First Steps",
        teaches: "Move, jump, reach the Goal",
        source: include_str!("../../assets/levels/01_first_steps.ron"),
    },
    BundledLevel {
        id: "02_glimmer_seal",
        name: "The Sealed Path",
        teaches: "Glimmers open Seals",
        source: include_str!("../../assets/levels/02_glimmer_seal.ron"),
    },
    BundledLevel {
        id: "03_launch_drift",
        name: "Air Mail",
        teaches: "Launch Pads & Drift Plates",
        source: include_str!("../../assets/levels/03_launch_drift.ron"),
    },
    BundledLevel {
        id: "04_prowler_tracks",
        name: "Patrol Route",
        teaches: "Prowlers ride tracks — stomp or dodge",
        source: include_str!("../../assets/levels/04_prowler_tracks.ron"),
    },
    BundledLevel {
        id: "05_channel_surfing",
        name: "Channel Surfing",
        teaches: "Trigger Orbs pulse Relay Gates",
        source: include_str!("../../assets/levels/05_channel_surfing.ron"),
    },
];

pub fn load_bundled(index: usize) -> Option<LevelData> {
    let entry = BUNDLED_LEVELS.get(index)?;
    match deserialize_level(entry.source) {
        Ok(data) => Some(data),
        Err(e) => {
            error!("Bundled level '{}' failed to parse: {e}", entry.id);
            None
        }
    }
}

/// Where the current level came from — controls mode defaults and Remix.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum LevelSource {
    #[default]
    Editor, // player's own level (starts in Edit)
    Bundled(usize), // campaign level (starts in Play, Remix available)
    Imported,       // from share code (starts in Play, Remix available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bundled_levels_parse_and_are_beatable_shaped() {
        assert!(BUNDLED_LEVELS.len() >= 3);
        for (i, b) in BUNDLED_LEVELS.iter().enumerate() {
            let data = match load_bundled(i) {
                Some(d) => d,
                None => {
                    let err = deserialize_level(b.source).unwrap_err();
                    panic!("{} must parse: {err}", b.id);
                }
            };
            assert!(!data.blocks.is_empty(), "{} has blocks", b.id);
            assert!(
                data.blocks
                    .iter()
                    .any(|blk| blk.kind == crate::maker::block::BlockKind::Goal),
                "{} has a Goal",
                b.id
            );
            assert!(data.is_verified, "{} is verified", b.id);
            assert!(data.author_time.is_some(), "{} has an author time", b.id);
            let spawn_under = [data.spawn[0], data.spawn[1] - 1, data.spawn[2]];
            assert!(
                data.blocks.iter().any(|blk| blk.position == spawn_under),
                "{} has a floor under its spawn",
                b.id
            );
        }
    }
}
