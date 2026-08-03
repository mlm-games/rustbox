use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use super::block::BlockKind;
use super::chunk::affected_chunks;
use super::entity_data::{EntityData, EntityDataExt, EntityKind, LevelEntityId};
use super::track::{TrackData, TrackDataExt, TrackId, TrackMode};

pub use rustbox_format::level::{
    BlockData, BoundaryConfig, BoundaryPreset, ClearCondition, LevelData, LevelTag, Theme,
};

/// Default fallback half-extents for levels with no explicit size.
const AUTO_SIZE_MIN: i32 = 8;
const AUTO_SIZE_MAX: i32 = 64;

#[derive(Resource, Clone, Debug)]
pub struct LevelDocument {
    pub data: LevelData,
    pub map: HashMap<IVec3, BlockData>,
    pub dirty_chunks: HashSet<IVec3>,
    pub next_entity_id: LevelEntityId,
    pub next_track_id: TrackId,
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
                tracks: vec![],
                entities_version: 1,
                author_time: None,
                author_deaths: 0,
                record_ms: None,
                clear_condition: ClearCondition::ReachGoal,
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
            },
            map: HashMap::new(),
            dirty_chunks: HashSet::new(),
            next_entity_id: 1,
            next_track_id: 1,
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
        self.data.is_verified = false;
        self.data.author_time = None;
        self.data.author_deaths = 0;
        self.data.clear_condition = ClearCondition::ReachGoal;
        self.data.size = None;
        self.data.water_level = None;
        self.data.theme = Theme::Grass;
        self.data.boundary = BoundaryConfig::default();
        self.data.secret_stars = 0;
        self.data.coin_star = false;

        for x in -8..=8 {
            for z in -8..=8 {
                self.set_block(
                    IVec3::new(x, 0, z),
                    Some(BlockData::new([x, 0, z], BlockKind::Grass)),
                );
            }
        }

        for x in 2..=5 {
            self.set_block(
                IVec3::new(x, 2, 0),
                Some(BlockData::new([x, 2, 0], BlockKind::Stone)),
            );
        }

        self.set_block(
            IVec3::new(0, 1, -3),
            Some(BlockData::new([0, 1, -3], BlockKind::Stone)),
        );
        self.set_block(
            IVec3::new(0, 2, -4),
            Some(BlockData::new([0, 2, -4], BlockKind::Stone)),
        );
        self.set_block(
            IVec3::new(0, 3, -5),
            Some(BlockData::new([0, 3, -5], BlockKind::Goal)),
        );

        self.data.entities.clear();
        self.data.tracks.clear();
        self.next_entity_id = 1;
        self.next_track_id = 1;

        let e1 =
            EntityData::defaults_for(EntityKind::Glimmer, IVec3::new(2, 1, 2), self.alloc_id());
        let e2 =
            EntityData::defaults_for(EntityKind::Glimmer, IVec3::new(-2, 1, 2), self.alloc_id());
        let e3 =
            EntityData::defaults_for(EntityKind::Glimmer, IVec3::new(0, 2, -2), self.alloc_id());
        let mut e4 =
            EntityData::defaults_for(EntityKind::Seal, IVec3::new(0, 1, -4), self.alloc_id());
        e4.param = 3.0;
        let mut e5 =
            EntityData::defaults_for(EntityKind::LaunchPad, IVec3::new(3, 1, -3), self.alloc_id());
        e5.yaw_deg = 180.0;
        e5.param = 16.0;
        let track_id = self.alloc_track_id();
        self.data.tracks.push(TrackData {
            id: track_id,
            points: vec![[-4, 2, -2], [-2, 2, -4], [-4, 2, -6]],
            mode: TrackMode::PingPong,
            speed: 2.5,
        });
        let mut e6 = EntityData::defaults_for(
            EntityKind::DriftPlate,
            IVec3::new(-4, 2, -2),
            self.alloc_id(),
        );
        e6.track = Some(track_id);
        e6.cell_b = None;
        e6.param = 2.5;
        for e in [e1, e2, e3, e4, e5, e6] {
            self.add_entity(e);
        }

        self.mark_all_dirty();
        self.rebuild_blocks_vec();
        self.entities_dirty = true;
    }

    pub fn get_block(&self, pos: IVec3) -> Option<&BlockData> {
        self.map.get(&pos)
    }

    pub fn get_kind(&self, pos: IVec3) -> Option<BlockKind> {
        self.map.get(&pos).map(|b| b.kind)
    }

    pub fn set_block(&mut self, pos: IVec3, data: Option<BlockData>) {
        match data {
            Some(data) => {
                if data.kind == BlockKind::Spawn {
                    self.data.spawn = [pos.x, pos.y + 1, pos.z];
                }
                self.map.insert(pos, data);
            }
            None => {
                self.map.remove(&pos);
            }
        }
        self.dirty_chunks.extend(affected_chunks(pos));
    }

    /// Effective play-area half-extents `[rx, ry, rz]`. `None` levels derive
    /// the bounds from their content with a margin, like MB64's grid.
    pub fn play_size(&self) -> [i32; 3] {
        match self.data.size {
            Some(s) => s,
            None => auto_size(&self.data),
        }
    }

    /// World height (in cells) the boundary walls rise to.
    pub fn boundary_top(&self) -> i32 {
        if self.data.boundary.height > 0 {
            self.data.boundary.height
        } else {
            self.play_size()[1]
        }
    }

    /// Whether `cell` is an invisible boundary solid (floor / walls / ceiling).
    pub fn boundary_solid(&self, cell: IVec3) -> bool {
        let size = self.play_size();
        let outside = cell.x.abs() > size[0] || cell.z.abs() > size[2];
        let top = self.boundary_top();
        let b = &self.data.boundary;
        // Full-height rim walls keep the player inside the play area.
        if b.inner_walls && outside && cell.y >= 0 && cell.y <= top {
            return true;
        }
        // Plateau lip: a low step ringing the level at its base.
        if b.outer_walls && outside && cell.y >= -1 && cell.y <= 0 {
            return true;
        }
        // Ceiling caps the room at the wall/room height.
        if b.ceiling && cell.y > top {
            return true;
        }
        // A catchable floor just below the play area, so you (usually) don't
        // fall into the void.
        if b.inner_floor && !outside && cell.y == -1 {
            return true;
        }
        if b.outer_floor && outside && cell.y == -1 {
            return true;
        }
        false
    }

    /// Whether `cell` is solid: either a placed block or the boundary.
    pub fn is_solid(&self, cell: IVec3) -> bool {
        self.get_block(cell)
            .map(|b| b.kind.is_solid())
            .unwrap_or(false)
            || self.boundary_solid(cell)
    }

    pub fn water_level(&self) -> Option<i32> {
        self.data.water_level
    }

    /// A cell is water-filled when the global plane covers it, when it holds a
    /// Water block, or when its block is waterlogged.
    pub fn cell_water(&self, cell: IVec3) -> bool {
        if self.data.water_level.is_some_and(|wl| cell.y < wl) {
            return true;
        }
        match self.get_block(cell) {
            Some(b) => b.kind == BlockKind::Water || b.waterlogged,
            None => false,
        }
    }

    pub fn is_underwater_point(&self, p: Vec3) -> bool {
        if self.data.water_level.is_some_and(|wl| p.y < wl as f32) {
            return true;
        }
        let cell = IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        self.cell_water(cell)
    }

    /// AABB of the playable volume `[min, max]` (inclusive cells).
    pub fn play_bounds(&self) -> (IVec3, IVec3) {
        let size = self.play_size();
        (
            IVec3::new(-size[0], 0, -size[2]),
            IVec3::new(size[0], self.boundary_top(), size[2]),
        )
    }

    /// AABB of the actually-placed blocks (ignoring boundary/walls). `None` for
    /// an empty level. Used for camera wrapping and content framing.
    pub fn content_bounds(&self) -> Option<(IVec3, IVec3)> {
        if self.map.is_empty() {
            return None;
        }
        let mut min = IVec3::splat(i32::MAX);
        let mut max = IVec3::splat(i32::MIN);
        for c in self.map.keys() {
            min = min.min(*c);
            max = max.max(*c);
        }
        Some((min, max))
    }

    pub fn mark_all_dirty(&mut self) {
        let chunks: HashSet<IVec3> = self
            .map
            .keys()
            .copied()
            .map(super::chunk::chunk_of)
            .collect();
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

    /// All entities occupying `cell` (in document order).
    pub fn entities_at_cell(&self, cell: IVec3) -> impl Iterator<Item = &EntityData> {
        self.data
            .entities
            .iter()
            .filter(move |e| e.cell_i() == cell)
    }

    /// The most-recently-added entity at `cell`, if any.
    pub fn top_entity_at_cell(&self, cell: IVec3) -> Option<&EntityData> {
        self.data.entities.iter().rev().find(|e| e.cell_i() == cell)
    }

    /// Whether an entity of `kind` may be placed at `cell` under stacking rules.
    pub fn can_place_entity_at(&self, cell: IVec3, kind: EntityKind) -> bool {
        if kind.stackable() {
            self.entities_at_cell(cell)
                .filter(|e| e.kind == kind)
                .count()
                < kind.max_stack()
        } else {
            self.entities_at_cell(cell).next().is_none()
        }
    }

    pub fn entity_by_id(&self, id: LevelEntityId) -> Option<&EntityData> {
        self.data.entities.iter().find(|e| e.id == id)
    }

    pub fn entity_mut(&mut self, id: LevelEntityId) -> Option<&mut EntityData> {
        self.entities_dirty = true;
        self.data.entities.iter_mut().find(|e| e.id == id)
    }

    pub fn alloc_track_id(&mut self) -> TrackId {
        let id = self.next_track_id;
        self.next_track_id += 1;
        id
    }

    pub fn track(&self, id: TrackId) -> Option<&TrackData> {
        self.data.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut TrackData> {
        self.entities_dirty = true;
        self.data.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn add_track(&mut self, track: TrackData) {
        self.next_track_id = self.next_track_id.max(track.id + 1);
        self.data.tracks.push(track);
        self.entities_dirty = true;
    }

    pub fn remove_track(&mut self, id: TrackId) -> Option<TrackData> {
        if let Some(i) = self.data.tracks.iter().position(|t| t.id == id) {
            let t = self.data.tracks.remove(i);
            self.entities_dirty = true;
            Some(t)
        } else {
            None
        }
    }

    /// Track whose nearest waypoint is at `cell`, if any.
    pub fn track_at_cell(&self, cell: IVec3) -> Option<TrackId> {
        self.data
            .tracks
            .iter()
            .find(|t| t.points.iter().any(|p| IVec3::from_array(*p) == cell))
            .map(|t| t.id)
    }

    /// Closest track to world point `p` within `radius` (geometric snap).
    pub fn track_near(&self, p: Vec3, radius: f32) -> Option<TrackId> {
        let mut best: Option<(f32, TrackId)> = None;
        for t in &self.data.tracks {
            if let Some((_, _, d)) = t.nearest(p)
                && d <= radius
                && best.as_ref().is_none_or(|(bd, _)| d < *bd)
            {
                best = Some((d, t.id));
            }
        }
        best.map(|(_, id)| id)
    }

    pub fn rebuild_blocks_vec(&mut self) {
        self.data.blocks = self
            .map
            .iter()
            .map(|(pos, data)| BlockData {
                position: [pos.x, pos.y, pos.z],
                kind: data.kind,
                shape: data.shape,
                rot: data.rot,
                waterlogged: data.waterlogged,
            })
            .collect();
        self.data
            .blocks
            .sort_by_key(|b| (b.position[1], b.position[0], b.position[2]));
    }

    /// Replace the whole document with `data` (used by bundled levels and
    /// imports), resetting derived state so the runtime rebuilds from scratch.
    pub fn replace_data(&mut self, data: LevelData) {
        self.map.clear();
        for b in &data.blocks {
            self.map.insert(
                IVec3::from_array(b.position),
                BlockData {
                    position: b.position,
                    kind: b.kind,
                    shape: b.shape,
                    rot: b.rot,
                    waterlogged: b.waterlogged,
                },
            );
        }
        self.data = data;
        self.next_entity_id = self.data.entities.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        self.next_track_id = self.data.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.rebuild_blocks_vec();
        self.mark_all_dirty();
        self.entities_dirty = true;
    }
}

fn auto_size(data: &LevelData) -> [i32; 3] {
    if data.blocks.is_empty() {
        return [AUTO_SIZE_MIN, AUTO_SIZE_MIN, AUTO_SIZE_MIN];
    }
    let mut max_abs_xz: i32 = 0;
    let mut max_y: i32 = 0;
    for b in &data.blocks {
        max_abs_xz = max_abs_xz.max(b.position[0].abs()).max(b.position[2].abs());
        max_y = max_y.max(b.position[1]);
    }
    let r = (max_abs_xz + 6).clamp(AUTO_SIZE_MIN, AUTO_SIZE_MAX);
    let ry = (max_y + 8).clamp(AUTO_SIZE_MIN, AUTO_SIZE_MAX);
    [r, ry, r]
}
