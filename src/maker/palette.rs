//! Stable UI palette indices for blocks/entities.
//! Icons, part picker, hotkeys, and UiAction mapping MUST use these helpers
//! so adding a kind never desyncs art ↔ gameplay.

use rustbox_format::{ALL_BLOCK_KINDS, ALL_ENTITY_KINDS, BlockKind, EntityKind};

pub fn block_index(kind: BlockKind) -> u8 {
    ALL_BLOCK_KINDS
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(0) as u8
}

pub fn block_from_index(i: u8) -> BlockKind {
    ALL_BLOCK_KINDS
        .get(i as usize)
        .copied()
        .unwrap_or(BlockKind::Grass)
}

pub fn entity_index(kind: EntityKind) -> u8 {
    ALL_ENTITY_KINDS
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(0) as u8
}

pub fn entity_from_index(i: u8) -> EntityKind {
    ALL_ENTITY_KINDS
        .get(i as usize)
        .copied()
        .unwrap_or(EntityKind::Glimmer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_roundtrip() {
        for (i, k) in ALL_BLOCK_KINDS.iter().enumerate() {
            assert_eq!(block_from_index(i as u8), *k);
            assert_eq!(block_index(*k) as usize, i);
        }
    }

    #[test]
    fn entity_roundtrip() {
        for (i, k) in ALL_ENTITY_KINDS.iter().enumerate() {
            assert_eq!(entity_from_index(i as u8), *k);
            assert_eq!(entity_index(*k) as usize, i);
        }
    }
}
