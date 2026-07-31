use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::chunk::affected_chunks;

use super::block::BlockKind;
use super::entity_data::{EntityData, EntityKind, LevelEntityId};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockData {
    pub position: [i32; 3],
    pub kind: BlockKind,
}

fn level_format_entities() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LevelData {
    pub name: String,
    pub spawn: [i32; 3],
    pub blocks: Vec<BlockData>,
    #[serde(default)]
    pub entities: Vec<EntityData>,
    #[serde(default = "level_format_entities")]
    pub entities_version: u32,
}

#[derive(Resource, Clone, Debug)]
pub struct LevelDocument {
    pub data: LevelData,
    pub map: HashMap<IVec3, BlockKind>,
    pub dirty_chunks: HashSet<IVec3>,
    pub next_entity_id: LevelEntityId,
    pub entities_dirty: bool,
}

impl Default for LevelDocument {
    fn default() -> Self {
        let mut doc = Self {
            data: LevelData {
                name: "Untitled Level".to_string(),
                spawn: [0, 2, 0],
                blocks: vec![],
                entities: vec![],
                entities_version: 1,
            },
            map: HashMap::new(),
            dirty_chunks: HashSet::new(),
            next_entity_id: 1,
            entities_dirty: true,
        };
        doc.seed_default();
        doc
    }
}

impl LevelDocument {
    pub fn seed_default(&mut self) {
        self.map.clear();
        self.data.blocks.clear();
        self.data.spawn = [0, 2, 0];

        for x in -8..=8 {
            for z in -8..=8 {
                self.set_block(IVec3::new(x, 0, z), Some(BlockKind::Grass));
            }
        }

        for x in 2..=5 {
            self.set_block(IVec3::new(x, 2, 0), Some(BlockKind::Stone));
        }

        self.set_block(IVec3::new(0, 1, -3), Some(BlockKind::Stone));
        self.set_block(IVec3::new(0, 2, -4), Some(BlockKind::Stone));
        self.set_block(IVec3::new(0, 3, -5), Some(BlockKind::Goal));

        self.data.entities.clear();
        self.next_entity_id = 1;

        let e1 = EntityData::defaults_for(EntityKind::Glimmer, IVec3::new(2, 1, 2), self.alloc_id());
        let e2 = EntityData::defaults_for(EntityKind::Glimmer, IVec3::new(-2, 1, 2), self.alloc_id());
        let e3 = EntityData::defaults_for(EntityKind::Glimmer, IVec3::new(0, 2, -2), self.alloc_id());
        let mut e4 = EntityData::defaults_for(EntityKind::Seal, IVec3::new(0, 1, -4), self.alloc_id());
        e4.param = 3.0;
        let mut e5 = EntityData::defaults_for(EntityKind::LaunchPad, IVec3::new(3, 1, -3), self.alloc_id());
        e5.yaw_deg = 180.0;
        e5.param = 16.0;
        let mut e6 = EntityData::defaults_for(EntityKind::DriftPlate, IVec3::new(-4, 2, -2), self.alloc_id());
        e6.cell_b = Some([-4, 2, -6]);
        e6.param = 2.5;
        for e in [e1, e2, e3, e4, e5, e6] {
            self.add_entity(e);
        }

        self.mark_all_dirty();
        self.rebuild_blocks_vec();
        self.entities_dirty = true;
    }

    pub fn get_block(&self, pos: IVec3) -> Option<BlockKind> {
        self.map.get(&pos).copied()
    }

    pub fn set_block(&mut self, pos: IVec3, kind: Option<BlockKind>) {
        match kind {
            Some(kind) => {
                self.map.insert(pos, kind);
                if kind == BlockKind::Spawn {
                    self.data.spawn = [pos.x, pos.y + 1, pos.z];
                }
            }
            None => {
                self.map.remove(&pos);
            }
        }
        self.dirty_chunks.extend(affected_chunks(pos));
    }

    pub fn mark_all_dirty(&mut self) {
        let chunks: HashSet<IVec3> =
            self.map.keys().copied().map(super::chunk::chunk_of).collect();
        self.dirty_chunks.extend(chunks);
    }

    pub fn alloc_id(&mut self) -> LevelEntityId {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        id
    }

    pub fn add_entity(&mut self, e: EntityData) {
        self.next_entity_id = self.next_entity_id.max(e.id + 1);
        self.data.entities.push(e);
        self.entities_dirty = true;
    }

    pub fn remove_entity(&mut self, id: LevelEntityId) -> Option<EntityData> {
        if let Some(i) = self.data.entities.iter().position(|e| e.id == id) {
            let e = self.data.entities.remove(i);
            self.entities_dirty = true;
            Some(e)
        } else {
            None
        }
    }

    pub fn entity_at_cell(&self, cell: IVec3) -> Option<&EntityData> {
        self.data.entities.iter().find(|e| e.cell_i() == cell)
    }

    pub fn rebuild_blocks_vec(&mut self) {
        self.data.blocks = self
            .map
            .iter()
            .map(|(pos, kind)| BlockData {
                position: [pos.x, pos.y, pos.z],
                kind: *kind,
            })
            .collect();
        self.data
            .blocks
            .sort_by_key(|b| (b.position[1], b.position[0], b.position[2]));
    }
}
