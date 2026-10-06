use std::collections::HashMap;

use bevy_ecs::prelude::*;
use glam::{IVec3, Mat4, Quat, Vec3};
use repame_shell::SimTime;
use repame_view3d::{Frame3d, MeshGroup};

use super::Paused;
use super::assets::{ModelAssets, push_instance, tint_instance};
use super::collision::is_solid;
use super::entity_data::{
    ContainedItem, EntityDataExt, EntityKind, EntityKindColor, LevelEntityId, link_color,
};
use super::interaction::{
    InteractionMemory, MAX_FAN_FORCE, cap_fan_force, contact_he, gateway_blocked, heal_allowed,
    player_overlaps_volume, solid_blocks,
};
use super::level::LevelDocument;
use super::level_view::srgb_to_linear;
use super::mode::MakerMode;
use super::player::{Player, PlayerTransform};
use super::props::{DriftPlate, RuntimeSolid, RuntimeSolids, SolidShape, Velocity};
use super::rapier::crate_body;
use super::screen::Trauma;
use super::track::{TrackDataExt, TrackId};
use super::win::MakerUi;
use repame_rapier3d::BodySnapshot3d;

#[derive(Resource, Default)]
pub struct EntityEntities(pub HashMap<LevelEntityId, Entity>);

#[derive(Component)]
pub struct LevelEnt {
    pub id: LevelEntityId,
    pub kind: EntityKind,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    pub fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    pub fn with_scale(mut self, scale: Vec3) -> Self {
        self.scale = scale;
        self
    }
}

#[derive(Component)]
pub struct GlimmerTag;

#[derive(Component)]
pub struct LaunchPad {
    pub impulse: f32,
    pub yaw_rad: f32,
}

#[derive(Component)]
pub struct Seal {
    pub need: u32,
    pub open: bool,
}

#[derive(Component)]
pub struct TrackFollower {
    pub track_id: TrackId,
    pub distance: f32,
    pub carry_player: bool,
}

#[derive(Component)]
pub struct Prowler {
    pub speed: f32,
    pub dir: Vec3,
    pub base_y: f32,
    pub prev: Vec3,
    pub on_track: bool,
}

#[derive(Component)]
pub struct Checkpoint {
    pub active: bool,
    pub respawn: Vec3,
}

#[derive(Component)]
pub struct SealSolid;

#[derive(Component)]
pub struct TriggerOrb {
    pub channel: u32,
    pub cooldown: f32,
    pub timer: f32,
}

#[derive(Component)]
pub struct RelayGate {
    pub channel: u32,
    pub duration: f32,
    pub open: bool,
    pub want_close: bool,
}

/// Non-Rapier solid marker for a closed gate.
#[derive(Component)]
pub struct GateSolid;

#[derive(Component)]
pub struct Teleporter {
    pub link: u32,
}

#[derive(Component)]
pub struct Fan {
    pub dir: Vec3,
    pub strength: f32,
}

#[derive(Component)]
pub struct Bumper {
    pub strength: f32,
}

#[derive(Component)]
pub struct CrateProp {
    pub breakable: bool,
}

/// Non-Rapier solid marker for a spawned wedge ramp. Collision derives from
/// `SolidShape::Wedge` (manifest), never from the visual mesh.
#[derive(Component)]
pub struct Wedge;

/// Stand-alone on/off switch. Touching it flips the global on/off state
/// (commit 18: toggles OnOffConveyorA/B).
#[derive(Component)]
pub struct OnOffSwitch;

/// A readable wooden signpost. Pressing the interact key while nearby and
/// facing it opens a dialog showing `text` (MB64 "Bill Board").
#[derive(Component)]
pub struct Sign {
    pub text: String,
    /// Facing direction (radians, Y-up), used for the read-facing check.
    pub yaw_rad: f32,
}

/// What a container (Crate / Prowler) will release when broken or defeated.
#[derive(Component)]
pub struct Contents {
    pub item: ContainedItem,
    /// Container's link channel - inherited by a contained Key.
    pub link: u32,
}

/// Runtime-spawned pickups (from broken crates / defeated prowlers) get ids in
/// this range so they can never collide with authored entity ids.
pub const DROP_ID_BASE: LevelEntityId = 0xF000_0000;

#[derive(Component)]
pub struct DroppedItem;

/// Simple pop-out ballistic. While present, the drop can't be picked up yet.
#[derive(Component)]
pub struct DropPop {
    pub vel: Vec3,
    pub rest_y: f32,
}

/// Dropped glimmers are self-contained; they don't route through the
/// authored-glimmer collection path at all.
#[derive(Component)]
pub struct DropGlimmer;

#[derive(Resource, Default)]
pub struct DropIdCounter(pub u32);

/// Spawns the pickups a container releases when broken (Crate) or defeated
/// (Prowler). Multiple items fan out in a circle; single items pop straight up.
pub fn spawn_drops(world: &mut World, origin: Vec3, contents: &Contents) {
    let items: Vec<(EntityKind, u32)> = match contents.item {
        ContainedItem::None => return,
        ContainedItem::Glimmers(n) => (0..n).map(|_| (EntityKind::Glimmer, 0)).collect(),
        ContainedItem::Key => vec![(EntityKind::Key, contents.link)],
        ContainedItem::HealOrb => vec![(EntityKind::HealOrb, 0)],
        ContainedItem::SpeedRing => vec![(EntityKind::SpeedRing, 0)],
    };

    let count = items.len().max(1) as f32;

    for (i, (kind, link)) in items.into_iter().enumerate() {
        let id = {
            let mut counter = world.resource_mut::<DropIdCounter>();
            counter.0 += 1;
            DROP_ID_BASE + counter.0
        };

        // Fan drops out in a circle; single items pop straight up.
        let angle = (i as f32 / count) * std::f32::consts::TAU;
        let spread = if count > 1.0 { 2.2 } else { 0.0 };
        let vel = Vec3::new(angle.cos() * spread, 7.5, angle.sin() * spread);

        let scale = match kind {
            EntityKind::Glimmer => 0.25,
            EntityKind::Key => 0.35,
            EntityKind::HealOrb => 0.35,
            EntityKind::SpeedRing => 0.7,
            _ => 0.3,
        };

        let eid = world
            .spawn((
                Transform::from_translation(origin + Vec3::Y * 0.4).with_scale(Vec3::splat(scale)),
                LevelEnt { id, kind },
                DroppedItem,
                DropPop {
                    vel,
                    rest_y: origin.y + 0.4,
                },
            ))
            .id();

        match kind {
            EntityKind::Glimmer => {
                world.entity_mut(eid).insert(DropGlimmer);
            }
            EntityKind::Key => {
                world.entity_mut(eid).insert(KeyPickup {
                    link: link.clamp(1, 9),
                });
            }
            EntityKind::HealOrb => {
                world.entity_mut(eid).insert(HealOrb);
            }
            EntityKind::SpeedRing => {
                world.entity_mut(eid).insert(SpeedRing { duration: 2.5 });
            }
            _ => {}
        }

        world.entity_mut(eid).insert(Sensor);
    }
}

#[derive(Component)]
pub struct KeyPickup {
    pub link: u32,
}

#[derive(Component)]
pub struct LockGate {
    pub link: u32,
    pub open: bool,
    /// 0 = stay open for the run once unlocked
    pub open_for: f32,
    pub open_timer: f32,
}

#[derive(Component)]
pub struct HealOrb;

#[derive(Component)]
pub struct SpeedRing {
    pub duration: f32,
}

#[derive(Component)]
pub struct CrumblePlate {
    pub delay: f32,
    pub timer: f32,
    pub triggered: bool,
    pub gone: bool,
}

/// Cannon: `cell_b` is the world target cell, `param` is the arc height.
#[derive(Component)]
pub struct Cannon {
    pub target: Vec3,
    pub arc: f32,
}

/// Simple procedural active visuals for kit entities: `base_y` anchors a bob,
/// `spin` is radians/sec, `bob` is bob amplitude.
#[derive(Component)]
pub struct KitAnim {
    pub base_y: f32,
    pub spin: f32,
    pub bob: f32,
    pub seed: f32,
}

/// Last pulse time per channel, in seconds of play-session time.
#[derive(Resource, Default)]
pub struct LinkState {
    pub pulses: std::collections::HashMap<u32, f32>,
    pub clock: f32,
}

/// A physics crate (TossCrate) that can be picked up and thrown (F).
#[derive(Component)]
pub struct Throwable;

/// Marks a crate currently held in front of the player (kinematic body).
#[derive(Component)]
pub struct Held;

#[derive(Component)]
pub struct Sensor;

#[derive(Component)]
pub struct DriftEndMarker;

/// Gameplay position of an entity's root transform (kept identical to the
/// pre-model values so hitboxes and proximity checks are unchanged.)
fn root_y_off(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Glimmer => 1.0,
        EntityKind::LaunchPad => 0.1,
        EntityKind::Seal => 1.0,
        EntityKind::DriftPlate => 0.15,
        EntityKind::Prowler => 0.4,
        EntityKind::TriggerOrb => 1.0,
        EntityKind::RelayGate => 1.0,
        EntityKind::Checkpoint => 0.55,
        EntityKind::Teleporter => 0.15,
        EntityKind::Fan => 0.5,
        EntityKind::Bumper => 0.35,
        EntityKind::Crate => 0.5,
        EntityKind::Key => 0.45,
        EntityKind::LockGate => 0.5,
        EntityKind::HealOrb => 0.45,
        EntityKind::SpeedRing => 0.55,
        EntityKind::CrumblePlate => 0.08,
        EntityKind::Cannon => 0.45,
        EntityKind::OnOffSwitch => 0.15,
        EntityKind::TossCrate => 0.5,
        EntityKind::Sign => 0.1,
        EntityKind::Wedge => 0.5,
    }
}

/// Lateral fan-out offsets so several entities sharing one cell don't
/// z-fight. Ring spreads outward; beyond the ring it just repeats the edge.
fn stack_offset(index: usize) -> Vec3 {
    let ring = [
        Vec3::ZERO,
        Vec3::new(0.22, 0.0, 0.0),
        Vec3::new(-0.22, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.22),
        Vec3::new(0.0, 0.0, -0.22),
        Vec3::new(0.16, 0.0, 0.16),
        Vec3::new(-0.16, 0.0, 0.16),
        Vec3::new(0.16, 0.0, -0.16),
    ];
    ring[index.min(ring.len() - 1)]
}

pub fn reconcile_entities(
    mut commands: Commands,
    mut level: ResMut<LevelDocument>,
    mut map: ResMut<EntityEntities>,
    mode: Res<MakerMode>,
) {
    if !level.entities_dirty && !mode.is_changed() {
        return;
    }
    level.entities_dirty = false;

    for (_, e) in map.0.drain() {
        commands.entity(e).despawn();
    }

    let playing = *mode == MakerMode::Play;

    // Per-cell index so stacked entities fan out instead of z-fighting.
    let mut cell_counts: std::collections::HashMap<IVec3, usize> = std::collections::HashMap::new();

    for data in &level.data.entities {
        let cell = data.cell_i();
        let stack_index = cell_counts.entry(cell).or_insert(0);
        let stack_offset = stack_offset(*stack_index);
        *stack_index += 1;

        let world = cell.as_vec3() + Vec3::new(0.5, 0.0, 0.5) + stack_offset;
        let yaw = data.yaw_deg.to_radians();
        let rot = Quat::from_rotation_y(yaw);

        let mut tf =
            Transform::from_translation(world + Vec3::Y * root_y_off(data.kind)).with_rotation(rot);
        let mut track_distance = 0.0;
        if let Some(track_id) = data.track
            && let Some((d, nearest, _)) = level.track(track_id).and_then(|t| t.nearest(world))
        {
            track_distance = d;
            tf.translation = nearest;
        }

        let eid = commands
            .spawn((
                tf,
                LevelEnt {
                    id: data.id,
                    kind: data.kind,
                },
            ))
            .id();

        let ecmds = &mut commands.entity(eid);

        match data.kind {
            EntityKind::Glimmer => {
                ecmds.insert(GlimmerTag);
                ecmds.insert(Sensor);
                ecmds.insert(KitAnim {
                    base_y: tf.translation.y,
                    spin: 2.0,
                    bob: 0.04,
                    seed: data.id as f32,
                });
            }
            EntityKind::LaunchPad => {
                ecmds.insert(LaunchPad {
                    impulse: data.param,
                    yaw_rad: yaw,
                });
            }
            EntityKind::Seal => {
                ecmds.insert(Seal {
                    need: data.param.max(1.0) as u32,
                    open: false,
                });
                ecmds.insert(SealSolid);
            }
            EntityKind::DriftPlate => {
                let a = data.cell_i().as_vec3() + Vec3::new(0.5, 0.15, 0.5);
                let b = data
                    .cell_b_i()
                    .unwrap_or(data.cell_i() + IVec3::new(4, 0, 0))
                    .as_vec3()
                    + Vec3::new(0.5, 0.15, 0.5);
                ecmds.insert(DriftPlate {
                    a,
                    b,
                    period: data.param.max(0.5),
                    t: 0.0,
                    carry: Vec3::ZERO,
                });
                if playing {
                    ecmds.insert(Velocity::zero());
                }
            }
            EntityKind::Prowler => {
                let yaw = data.yaw_deg.to_radians();
                let dir = (Quat::from_rotation_y(yaw) * Vec3::NEG_Z).normalize();
                let world = data.cell_i().as_vec3() + Vec3::new(0.5, 0.4, 0.5);
                if data.track.is_none() {
                    tf.translation = world;
                    tf.rotation = Quat::from_rotation_y(yaw);
                }
                ecmds.insert(Prowler {
                    speed: data.param.max(0.1),
                    dir,
                    base_y: world.y,
                    prev: tf.translation,
                    on_track: data.track.is_some(),
                });
                ecmds.insert(Contents {
                    item: data.contents,
                    link: data.link,
                });
            }
            EntityKind::TriggerOrb => {
                ecmds.insert(TriggerOrb {
                    channel: data.link,
                    cooldown: data.param.max(0.2),
                    timer: 0.0,
                });
                ecmds.insert(Sensor);
            }
            EntityKind::RelayGate => {
                ecmds.insert(RelayGate {
                    channel: data.link,
                    duration: data.param.max(0.5),
                    open: false,
                    want_close: false,
                });
                ecmds.insert(GateSolid);
            }
            EntityKind::Checkpoint => {
                let cell = data.cell_i();
                ecmds.insert(Checkpoint {
                    active: false,
                    respawn: Vec3::new(
                        cell.x as f32 + 0.5,
                        cell.y as f32 + 1.4,
                        cell.z as f32 + 0.5,
                    ),
                });
                ecmds.insert(Sensor);
            }
            EntityKind::Teleporter => {
                ecmds.insert(Teleporter { link: data.link });
                ecmds.insert(Sensor);
            }
            EntityKind::Fan => {
                let yaw = data.yaw_deg.to_radians();
                // Match prowler / camera forward: local -Z after yaw.
                let dir = (Quat::from_rotation_y(yaw) * Vec3::NEG_Z).normalize_or_zero();
                ecmds.insert(Fan {
                    dir,
                    strength: data.param.max(0.0),
                });
                ecmds.insert(KitAnim {
                    base_y: tf.translation.y,
                    spin: 2.5,
                    bob: 0.0,
                    seed: data.id as f32,
                });
            }
            EntityKind::Bumper => {
                ecmds.insert(Bumper {
                    strength: data.param.max(1.0),
                });
                ecmds.insert(Sensor);
                ecmds.insert(KitAnim {
                    base_y: tf.translation.y,
                    spin: 0.0,
                    bob: 0.0,
                    seed: data.id as f32,
                });
            }
            EntityKind::Crate => {
                ecmds.insert(CrateProp {
                    breakable: data.param >= 0.5,
                });
                ecmds.insert(Contents {
                    item: data.contents,
                    link: data.link,
                });
            }
            EntityKind::Key => {
                ecmds.insert(KeyPickup {
                    link: data.link.max(1).min(9),
                });
                ecmds.insert(Sensor);
                ecmds.insert(KitAnim {
                    base_y: tf.translation.y,
                    spin: 2.0,
                    bob: 0.08,
                    seed: data.id as f32,
                });
            }
            EntityKind::LockGate => {
                ecmds.insert(LockGate {
                    link: data.link.max(1).min(9),
                    open: false,
                    open_for: data.param.max(0.0),
                    open_timer: 0.0,
                });
            }
            EntityKind::HealOrb => {
                ecmds.insert(HealOrb);
                ecmds.insert(Sensor);
                ecmds.insert(KitAnim {
                    base_y: tf.translation.y,
                    spin: 1.5,
                    bob: 0.08,
                    seed: data.id as f32,
                });
            }
            EntityKind::SpeedRing => {
                ecmds.insert(SpeedRing {
                    duration: data.param.max(0.25),
                });
                ecmds.insert(Sensor);
                ecmds.insert(KitAnim {
                    base_y: tf.translation.y,
                    spin: 2.0,
                    bob: 0.0,
                    seed: data.id as f32,
                });
            }
            EntityKind::CrumblePlate => {
                ecmds.insert(CrumblePlate {
                    delay: data.param.max(0.05),
                    timer: 0.0,
                    triggered: false,
                    gone: false,
                });
            }
            EntityKind::Cannon => {
                let from = data.cell_i().as_vec3() + Vec3::new(0.5, 0.45, 0.5);
                let target = data
                    .cell_b_i()
                    .unwrap_or(data.cell_i() + IVec3::new(4, 0, 0))
                    .as_vec3()
                    + Vec3::new(0.5, 0.45, 0.5);
                ecmds.insert(Cannon {
                    target: (target - from) * Vec3::new(1.0, 0.0, 1.0) + from,
                    arc: data.param.max(1.0),
                });
                ecmds.insert(Sensor);
                ecmds.insert(KitAnim {
                    base_y: tf.translation.y,
                    spin: 1.0,
                    bob: 0.0,
                    seed: data.id as f32,
                });
            }
            EntityKind::OnOffSwitch => {
                ecmds.insert(OnOffSwitch);
                ecmds.insert(Sensor);
            }
            EntityKind::Sign => {
                ecmds.insert(Sign {
                    text: data.sign_text.clone(),
                    yaw_rad: data.yaw_deg.to_radians(),
                });
                ecmds.insert(Sensor);
            }
            EntityKind::Wedge => {
                ecmds.insert(Wedge);
            }
            EntityKind::TossCrate => {
                ecmds.insert(CrateProp {
                    breakable: data.param >= 0.5,
                });
                ecmds.insert(Contents {
                    item: data.contents,
                    link: data.link,
                });
                if playing {
                    ecmds.insert(Throwable);
                    ecmds.insert(Velocity::zero());
                    ecmds.insert(BodySnapshot3d::default());
                    ecmds.insert(crate_body(tf.translation, tf.rotation));
                }
            }
        }

        if let Some(track_id) = data.track {
            ecmds.insert(TrackFollower {
                track_id,
                distance: track_distance,
                carry_player: data.kind == EntityKind::DriftPlate,
            });
        }

        if !playing
            && data.kind == EntityKind::DriftPlate
            && data.track.is_none()
            && let Some(b) = data.cell_b_i()
        {
            let b = b.as_vec3() + Vec3::new(0.5, 0.15, 0.5);
            commands.spawn((
                Transform::from_translation(b).with_scale(Vec3::splat(0.3)),
                DriftEndMarker,
            ));
        }

        map.0.insert(data.id, eid);
    }
}

/// Drops are run-scoped: whenever the entity layer rebuilds (mode change,
/// retry), clear them. Must run BEFORE `reconcile_entities` clears the flag.
pub fn despawn_drops_when_dirty(
    mut commands: Commands,
    level: Res<LevelDocument>,
    mode: Res<MakerMode>,
    mut counter: ResMut<DropIdCounter>,
    drops: Query<Entity, With<DroppedItem>>,
) {
    if !level.entities_dirty && !mode.is_changed() {
        return;
    }
    for e in &drops {
        commands.entity(e).despawn();
    }
    counter.0 = 0;
}

pub fn update_drops(world: &mut World, dt: f32) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let mut finished: Vec<Entity> = Vec::new();
    {
        let mut q =
            world.query_filtered::<(Entity, &mut Transform, &mut DropPop), With<DroppedItem>>();
        for (e, mut tf, mut pop) in q.iter_mut(world) {
            pop.vel.y -= 22.0 * dt;
            tf.translation += pop.vel * dt;
            if pop.vel.y < 0.0 && tf.translation.y <= pop.rest_y {
                tf.translation.y = pop.rest_y;
                finished.push(e);
            }
        }
    }
    for e in finished {
        world.entity_mut(e).remove::<DropPop>();
    }
}

pub fn collect_dropped_glimmers(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let pt = {
        let mut q = world.query_filtered::<&PlayerTransform, With<Player>>();
        let Ok(pt) = q.single(world) else {
            return;
        };
        pt.translation
    };
    let mut to_despawn: Vec<Entity> = Vec::new();
    {
        let mut q =
            world.query_filtered::<(Entity, &Transform), (With<DropGlimmer>, Without<DropPop>)>();
        for (e, tf) in q.iter(world) {
            if pt.distance(tf.translation) > 1.0 {
                continue;
            }
            to_despawn.push(e);
        }
    }
    for e in to_despawn {
        world.despawn(e);
        let (c, t) = {
            let mut ui = world.resource_mut::<MakerUi>();
            ui.glimmers_collected += 1;
            ui.score += 100;
            (ui.glimmers_collected, ui.glimmers_total)
        };
        world
            .resource_mut::<MakerUi>()
            .set_status(format!("Glimmer {c}/{t}"));
    }
}

pub fn wrap_sign_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split('\n') {
        let mut line = String::new();
        for ch in raw.chars() {
            if ch == '\r' {
                continue;
            }
            if line.chars().count() >= 30 {
                out.push(std::mem::take(&mut line));
            }
            line.push(ch);
        }
        out.push(line);
    }
    out
}

pub fn touch_checkpoints(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }

    let (pt, checkpoint_id) = {
        let mut q = world.query_filtered::<(&PlayerTransform, &Player), With<Player>>();
        let Ok((pt, player)) = q.single(world) else {
            return;
        };
        (pt.translation, player.checkpoint_id)
    };

    let mut hit: Option<(LevelEntityId, Vec3)> = None;

    {
        let mut q = world.query::<(&LevelEnt, &Transform, &Checkpoint)>();
        for (ent, tf, cp) in q.iter(world) {
            if pt.distance(tf.translation) < 1.2 {
                if checkpoint_id != Some(ent.id) {
                    hit = Some((ent.id, cp.respawn));
                }
                break;
            }
        }
    }

    let Some((new_id, respawn)) = hit else {
        return;
    };

    {
        let mut q = world.query_filtered::<&mut Player, With<Player>>();
        let Ok(mut player) = q.single_mut(world) else {
            return;
        };
        player.checkpoint_id = Some(new_id);
        player.respawn_point = respawn;
    }

    {
        let mut q = world.query::<(&LevelEnt, &mut Checkpoint)>();
        for (ent, mut cp) in q.iter_mut(world) {
            cp.active = ent.id == new_id;
        }
    }

    world
        .resource_mut::<MakerUi>()
        .set_status("Checkpoint reached!");
}

pub fn collect_glimmers(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let pt = {
        let mut q = world.query_filtered::<&PlayerTransform, With<Player>>();
        let Ok(pt) = q.single(world) else {
            return;
        };
        pt.translation
    };
    let mut to_despawn: Vec<Entity> = Vec::new();
    {
        let mut q = world.query_filtered::<(Entity, &Transform), With<GlimmerTag>>();
        for (e, gt) in q.iter(world) {
            if pt.distance(gt.translation) < 1.0 {
                to_despawn.push(e);
            }
        }
    }
    let count = to_despawn.len() as u32;
    for e in to_despawn {
        world.despawn(e);
    }
    if count > 0 {
        let mut ui = world.resource_mut::<MakerUi>();
        let total = ui.glimmers_collected + count;
        ui.glimmers_collected = total;
        ui.set_status(format!("Glimmer x{total}"));
        drop(ui);
        world.resource_mut::<Trauma>().add(0.12 * count as f32);
    }
}

pub fn update_seals(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let to_open: Vec<Entity> = {
        let mut q = world.query::<(Entity, &Seal)>();
        let ui = world.resource::<MakerUi>();
        q.iter(world)
            .filter(|(_, seal)| ui.glimmers_collected >= seal.need && !seal.open)
            .map(|(e, _)| e)
            .collect()
    };
    for e in to_open {
        if let Some(mut seal) = world.get_mut::<Seal>(e) {
            seal.open = true;
        }
        if world.get::<SealSolid>(e).is_some() {
            world.entity_mut(e).remove::<SealSolid>();
        }
    }
}

/// Gates open while (clock - last_pulse) < duration; close crush-safe, waiting
/// for every body (player / crate / prowler) to clear the doorway.
pub fn update_relay_gates(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let mut bodies: Vec<(Vec3, Vec3)> = Vec::new();
    {
        let mut q = world.query_filtered::<(&PlayerTransform, &Player), With<Player>>();
        if let Ok((pt, p)) = q.single(world) {
            bodies.push((pt.translation, contact_he(p)));
        }
    }
    {
        let mut q = world.query_filtered::<&Transform, With<Prowler>>();
        for t in q.iter(world) {
            bodies.push((t.translation, Vec3::splat(0.35)));
        }
    }
    {
        let mut q = world.query_filtered::<&Transform, With<CrateProp>>();
        for t in q.iter(world) {
            bodies.push((t.translation, Vec3::splat(0.5)));
        }
    }
    let (pulses, clock) = {
        let link = world.resource::<LinkState>();
        (link.pulses.clone(), link.clock)
    };
    let solids = RuntimeSolids {
        solids: world.resource::<RuntimeSolids>().solids.clone(),
    };
    let mut actions: Vec<(Entity, bool)> = Vec::new();
    {
        let mut q = world.query::<(Entity, &LevelEnt, &Transform, &mut RelayGate)>();
        for (e, le, gt, mut gate) in q.iter_mut(world) {
            let powered = gate.channel != 0
                && pulses
                    .get(&gate.channel)
                    .is_some_and(|t| clock - t < gate.duration);

            if powered && !gate.open {
                gate.open = true;
                gate.want_close = false;
                actions.push((e, true));
            } else if !powered && gate.open {
                // Crush-safe close: wait until nothing (body or solid) is in the
                // doorway. A gate never blocks its own doorway.
                if gateway_blocked(
                    bodies.iter().copied(),
                    gt.translation,
                    Vec3::new(0.5, 1.0, 0.2),
                ) || solid_blocks(&solids, le.id, gt.translation, Vec3::new(0.5, 1.0, 0.2))
                {
                    gate.want_close = true;
                } else {
                    gate.open = false;
                    gate.want_close = false;
                    actions.push((e, false));
                }
            }
        }
    }
    for (e, open) in actions {
        if open {
            world.entity_mut(e).remove::<GateSolid>();
        } else {
            world.entity_mut(e).insert(GateSolid);
        }
    }
}

pub fn tick_drift_plates(
    time: Res<SimTime>,
    _mode: Res<MakerMode>,
    mut plates: Query<
        (&mut Transform, &mut DriftPlate, Option<&mut Velocity>),
        Without<TrackFollower>,
    >,
) {
    let dt = time.delta_secs;
    for (mut tf, mut drift, vel) in &mut plates {
        let prev = tf.translation;
        drift.t = (drift.t + dt) % (drift.period * 2.0);
        let phase = if drift.t <= drift.period {
            drift.t / drift.period
        } else {
            1.0 - (drift.t - drift.period) / drift.period
        };
        let s = phase * phase * (3.0 - 2.0 * phase);
        tf.translation = drift.a.lerp(drift.b, s);
        drift.carry = tf.translation - prev;
        if let Some(mut vel) = vel {
            vel.linear = if dt > 0.0 {
                drift.carry / dt
            } else {
                Vec3::ZERO
            };
        }
    }
}

pub fn tick_track_followers(
    time: Res<SimTime>,
    level: Res<LevelDocument>,
    _mode: Res<MakerMode>,
    mut followers: Query<(
        &mut Transform,
        &mut TrackFollower,
        Option<&mut DriftPlate>,
        Option<&mut Velocity>,
    )>,
) {
    let dt = time.delta_secs;
    for (mut tf, mut follow, drift, vel) in &mut followers {
        let Some(track) = level.track(follow.track_id) else {
            continue;
        };
        let prev = tf.translation;
        follow.distance += dt * track.speed.max(0.0);
        tf.translation = track.sample(follow.distance);
        let step = tf.translation - prev;
        if let Some(mut drift) = drift {
            if follow.carry_player {
                drift.carry = step;
                if let Some(mut vel) = vel {
                    vel.linear = if dt > 0.0 { step / dt } else { Vec3::ZERO };
                }
            } else {
                drift.carry = Vec3::ZERO;
                if let Some(mut vel) = vel {
                    vel.linear = Vec3::ZERO;
                }
            }
        } else if let Some(mut vel) = vel {
            vel.linear = if dt > 0.0 { step / dt } else { Vec3::ZERO };
        }
    }
}

pub fn move_prowlers(
    time: Res<SimTime>,
    mode: Res<MakerMode>,
    level: Res<LevelDocument>,
    solids: Res<RuntimeSolids>,
    plates: Query<(&Transform, &DriftPlate, Option<&Velocity>), Without<Prowler>>,
    mut q: Query<(&LevelEnt, &mut Transform, &mut Prowler)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let dt = time.delta_secs;

    let mut steps: Vec<(Vec3, Vec3, Vec3)> = Vec::new();
    if dt > 0.0 {
        for (ptf, drift, vel) in &plates {
            let step = vel
                .map(|v| v.linear * dt)
                .filter(|s| s.length_squared() > 1e-12)
                .unwrap_or(drift.carry);
            if step.length_squared() > 1e-12 {
                steps.push((ptf.translation, step, Vec3::new(0.7, 0.12, 0.7)));
            }
        }
    }
    let ride_step_at = |pos: Vec3, he: Vec3| -> Vec3 {
        let feet = pos.y - he.y;
        for (center, step, phe) in &steps {
            let top = center.y + phe.y;
            if (pos.x - center.x).abs() < phe.x + he.x + 0.1
                && (pos.z - center.z).abs() < phe.z + he.z + 0.1
                && (feet - top).abs() <= 0.30
            {
                return *step;
            }
        }
        Vec3::ZERO
    };

    for (le, mut tf, mut p) in &mut q {
        if p.on_track {
            let delta = tf.translation - p.prev;
            let flat = Vec3::new(delta.x, 0.0, delta.z);
            if flat.length_squared() > 1e-6 {
                p.dir = flat.normalize();
                tf.rotation = Quat::from_rotation_y((-p.dir.x).atan2(-p.dir.z));
            }
            p.prev = tf.translation;
            continue;
        }

        let ride = ride_step_at(tf.translation, Vec3::splat(0.35));
        if ride.length_squared() > 1e-12 {
            tf.translation += ride;
            p.base_y += ride.y;
            p.prev += ride;
        }

        let step = p.dir * p.speed * dt;
        let next = Vec3::new(
            tf.translation.x + step.x,
            p.base_y,
            tf.translation.z + step.z,
        );

        let ahead = next + p.dir * 0.35;
        let body_y = p.base_y.floor() as i32;
        let ahead_cell = IVec3::new(ahead.x.floor() as i32, body_y, ahead.z.floor() as i32);
        let wall = is_solid(&level, ahead_cell);
        let ledge = !is_solid(&level, ahead_cell - IVec3::Y);
        // Closed gates/seals/crates are entity solids, not level cells:
        // without this prowlers walk straight through them.
        let blocked_by_prop = solid_blocks(&solids, le.id, next, Vec3::splat(0.35));
        let headroom = is_solid(&level, ahead_cell + IVec3::Y);

        if wall || ledge || blocked_by_prop || headroom {
            p.dir = -p.dir;
        } else {
            tf.translation = next;
        }
        tf.rotation = Quat::from_rotation_y((-p.dir.x).atan2(-p.dir.z));
    }
}

/// Static crates resting on a rideable platform travel with its full XYZ step.
/// Dynamic (thrown/held) crates ride through Rapier friction via the platform
/// velocity instead and are skipped here.
pub fn carry_crate_riders(
    time: Res<SimTime>,
    mode: Res<MakerMode>,
    plates: Query<(&Transform, &DriftPlate, Option<&Velocity>), Without<CrateProp>>,
    mut crates: Query<&mut Transform, (With<CrateProp>, Without<Prowler>, Without<Held>)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let dt = time.delta_secs;
    if dt <= 0.0 {
        return;
    }
    let mut steps: Vec<(Vec3, Vec3)> = Vec::new();
    for (ptf, drift, vel) in &plates {
        let step = vel
            .map(|v| v.linear * dt)
            .filter(|s| s.length_squared() > 1e-12)
            .unwrap_or(drift.carry);
        if step.length_squared() > 1e-12 {
            steps.push((ptf.translation, step));
        }
    }
    if steps.is_empty() {
        return;
    }
    for mut tf in &mut crates {
        let feet = tf.translation.y - 0.4;
        for (center, step) in &steps {
            let top = center.y + 0.12;
            if (tf.translation.x - center.x).abs() < 0.7 + 0.4 + 0.1
                && (tf.translation.z - center.z).abs() < 0.7 + 0.4 + 0.1
                && (feet - top).abs() <= 0.30
            {
                tf.translation += *step;
                break;
            }
        }
    }
}

pub fn rebuild_runtime_solids(world: &mut World) {
    let seals: Vec<(LevelEntityId, Vec3, Quat, bool)> = {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform, &Seal), With<SealSolid>>();
        q.iter(world)
            .map(|(le, t, s)| (le.id, t.translation, t.rotation, s.open))
            .collect()
    };
    let gates: Vec<(LevelEntityId, Vec3, Quat, bool)> = {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform, &RelayGate), With<GateSolid>>();
        q.iter(world)
            .map(|(le, t, g)| (le.id, t.translation, t.rotation, g.open))
            .collect()
    };
    let lock_gates: Vec<(LevelEntityId, Vec3, Quat, bool)> = {
        let mut q = world.query::<(&LevelEnt, &Transform, &LockGate)>();
        q.iter(world)
            .map(|(le, t, l)| (le.id, t.translation, t.rotation, l.open))
            .collect()
    };
    let crates: Vec<(LevelEntityId, Vec3, Quat)> = {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform, &CrateProp), Without<Held>>();
        q.iter(world)
            .map(|(le, t, _)| (le.id, t.translation, t.rotation))
            .collect()
    };
    let plates: Vec<(LevelEntityId, Vec3, Quat, bool)> = {
        let mut q = world.query::<(&LevelEnt, &Transform, &CrumblePlate)>();
        q.iter(world)
            .map(|(le, t, p)| (le.id, t.translation, t.rotation, p.gone))
            .collect()
    };
    let wedges: Vec<(LevelEntityId, Vec3, Quat)> = {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<Wedge>>();
        q.iter(world)
            .map(|(le, t)| (le.id, t.translation, t.rotation))
            .collect()
    };
    let pads: Vec<(LevelEntityId, Vec3, Quat)> = {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<LaunchPad>>();
        q.iter(world)
            .map(|(le, t)| (le.id, t.translation, t.rotation))
            .collect()
    };
    let drift: Vec<(LevelEntityId, Vec3, Quat)> = {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<DriftPlate>>();
        q.iter(world)
            .map(|(le, t)| (le.id, t.translation, t.rotation))
            .collect()
    };
    let mut out = build_solids(seals, gates, lock_gates, crates, plates, wedges, pads);
    // Drift plates are moving platforms: give the custom collider their real
    // footprint so the player lands on / is stopped by them, not just the
    // special-case ride. Matches the rapier cuboid(0.7, 0.12, 0.7).
    for (le, center, rotation) in &drift {
        out.push(RuntimeSolid {
            owner: *le,
            center: *center,
            shape: SolidShape::Box(0.7, 0.12, 0.7),
            rotation: *rotation,
        });
    }
    world.resource_mut::<RuntimeSolids>().solids = out;
}

/// Pure solid-table builder (shared with tests). Collision state derives from
/// authoritative open/gone flags, never from visibility.
pub fn build_solids(
    seals: Vec<(LevelEntityId, Vec3, Quat, bool)>,
    gates: Vec<(LevelEntityId, Vec3, Quat, bool)>,
    lock_gates: Vec<(LevelEntityId, Vec3, Quat, bool)>,
    crates: Vec<(LevelEntityId, Vec3, Quat)>,
    plates: Vec<(LevelEntityId, Vec3, Quat, bool)>,
    wedges: Vec<(LevelEntityId, Vec3, Quat)>,
    pads: Vec<(LevelEntityId, Vec3, Quat)>,
) -> Vec<RuntimeSolid> {
    let mut out = Vec::new();
    for (e, center, rotation, open) in seals {
        if !open {
            out.push(RuntimeSolid {
                owner: e,
                center,
                shape: SolidShape::Box(0.5, 1.0, 0.15),
                rotation,
            });
        }
    }
    for (e, center, rotation, open) in gates {
        if !open {
            out.push(RuntimeSolid {
                owner: e,
                center,
                shape: SolidShape::Box(0.5, 1.0, 0.2),
                rotation,
            });
        }
    }
    for (e, center, rotation, open) in lock_gates {
        if !open {
            out.push(RuntimeSolid {
                owner: e,
                center,
                shape: SolidShape::Box(0.55, 1.2, 0.3),
                rotation,
            });
        }
    }
    for (e, center, rotation) in crates {
        out.push(RuntimeSolid {
            owner: e,
            center,
            shape: SolidShape::Box(0.4, 0.4, 0.4),
            rotation,
        });
    }
    for (e, center, rotation, gone) in plates {
        if !gone {
            out.push(RuntimeSolid {
                owner: e,
                center,
                shape: SolidShape::Box(0.5, 0.12, 0.5),
                rotation,
            });
        }
    }
    for (e, center, rotation) in wedges {
        out.push(RuntimeSolid {
            owner: e,
            center,
            shape: SolidShape::Wedge(0.5, 0.5, 0.5),
            rotation,
        });
    }
    // Launch pads: thin standable top so TopContact footing and the custom
    // mover agree (the Rapier collider on pads is never used by the player).
    // Box top lands at center.y + 0.05 + 0.1 = root.y + 0.15, matching
    // `PAD_TOP_OFFSET` in interaction.rs.
    for (e, center, rotation) in pads {
        out.push(RuntimeSolid {
            owner: e,
            center: center + Vec3::Y * 0.05,
            shape: SolidShape::Box(0.45, 0.1, 0.45),
            rotation,
        });
    }
    out
}

/// Fans accumulate a capped force. Continuous fans apply at the start of
/// PlayerMotion (before the controller), independent of forced-motion state
/// (which `begin_interaction_frame` clears each frame).
pub fn apply_fans(
    time: Res<SimTime>,
    mode: Res<MakerMode>,
    mut player_q: Query<(&PlayerTransform, &mut Player)>,
    fans: Query<(&Transform, &Fan), Without<Player>>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let Ok((pt, mut player)) = player_q.single_mut() else {
        return;
    };
    let dt = time.delta_secs;

    let mut force = Vec3::ZERO;
    for (tf, fan) in &fans {
        let to_p = pt.translation - tf.translation;
        let ahead = to_p.dot(fan.dir);
        if !(0.0..=4.0).contains(&ahead) {
            continue;
        }
        let lateral = (to_p - fan.dir * ahead).length();
        if lateral > 1.4 {
            continue;
        }
        // Falloff so standing at the rim isn't full blast.
        let falloff = (1.0 - ahead / 4.0).clamp(0.0, 1.0);
        force += fan.dir * fan.strength * falloff * dt;
        // Slight lift so fans feel useful in 3D platforming.
        force.y += fan.strength * 0.15 * falloff * dt;
    }
    // `force` is a per-tick velocity delta (already ×dt); the cap is a
    // velocity rate (m/s), so scale it to the tick. Without this a single fan
    // (~0.2/tick) can never reach the cap and 10 stacked fans (~2.0) still
    // pass, integrating unbounded every FixedUpdate.
    player.velocity += cap_fan_force(force, MAX_FAN_FORCE * dt.max(1e-4));
}

pub fn collect_keys(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let (pt, he, player_e) = {
        let mut q = world.query_filtered::<(Entity, &PlayerTransform, &Player), With<Player>>();
        let Ok((e, pt, player)) = q.single(world) else {
            return;
        };
        (pt.translation, player.half_extents, e)
    };
    let mut hits: Vec<(Entity, LevelEntityId, usize)> = Vec::new();
    {
        let mut q = world.query_filtered::<
            (Entity, &Transform, &LevelEnt, &KeyPickup),
            (Without<Player>, Without<DropPop>),
        >();
        for (e, tf, ent, key) in q.iter(world) {
            if !player_overlaps_volume(pt, he, tf.translation, Vec3::splat(0.5)) {
                continue;
            }
            hits.push((e, ent.id, key.link as usize));
        }
    }
    if hits.is_empty() {
        return;
    }
    {
        let mut q = world.query::<&mut Player>();
        if let Ok(mut player) = q.get_mut(world, player_e) {
            for (_, _, ch) in &hits {
                if *ch < player.keys.len() {
                    player.keys[*ch] = player.keys[*ch].saturating_add(1);
                }
            }
        }
    }
    for (e, id, ch) in hits {
        world.despawn(e);
        world.resource_mut::<EntityEntities>().0.remove(&id);
        world
            .resource_mut::<MakerUi>()
            .set_status(format!("Key (ch {ch})"));
    }
}

/// Lock gates unlock only through the arbitrated explicit use target
/// (`interaction::resolve_use`). This system handles timed closing and re-arms
/// the gate; closing waits until the doorway is clear, so it never consumes a
/// second key merely because a timed gate closed while the player stayed near.
pub fn update_lock_gates(world: &mut World, dt: f32) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let mut bodies: Vec<(Vec3, Vec3)> = Vec::new();
    {
        let mut q = world.query_filtered::<(&PlayerTransform, &Player), With<Player>>();
        if let Ok((pt, p)) = q.single(world) {
            bodies.push((pt.translation, contact_he(p)));
        }
    }
    {
        let mut q = world.query_filtered::<&Transform, With<Prowler>>();
        for t in q.iter(world) {
            bodies.push((t.translation, Vec3::splat(0.35)));
        }
    }
    {
        let mut q = world.query_filtered::<&Transform, With<CrateProp>>();
        for t in q.iter(world) {
            bodies.push((t.translation, Vec3::splat(0.5)));
        }
    }
    let solids = RuntimeSolids {
        solids: world.resource::<RuntimeSolids>().solids.clone(),
    };

    let mut q = world.query::<(&LevelEnt, &Transform, &mut LockGate)>();
    for (le, tf, mut gate) in q.iter_mut(world) {
        if !gate.open {
            continue;
        }
        if gate.open_for <= 0.0 {
            continue;
        }
        gate.open_timer -= dt;
        if gate.open_timer <= 0.0
            && !gateway_blocked(
                bodies.iter().copied(),
                tf.translation,
                Vec3::new(0.55, 1.2, 0.3),
            )
            && !solid_blocks(&solids, le.id, tf.translation, Vec3::new(0.55, 1.2, 0.3))
        {
            gate.open = false;
        }
    }
}

/// Heal orbs use shape overlap and are only consumed when armor is below the
/// cap. Drop-pop animation still blocks pickup via `Without<DropPop>`.
pub fn collect_heal_orbs(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let (pt, he, player_e) = {
        let mut q = world.query_filtered::<(Entity, &PlayerTransform, &Player), With<Player>>();
        let Ok((e, pt, player)) = q.single(world) else {
            return;
        };
        (pt.translation, player.half_extents, e)
    };
    let mut hits: Vec<(Entity, LevelEntityId)> = Vec::new();
    {
        let mut q = world.query_filtered::<
            (Entity, &Transform, &LevelEnt),
            (With<HealOrb>, Without<Player>, Without<DropPop>),
        >();
        for (e, tf, ent) in q.iter(world) {
            if !player_overlaps_volume(pt, he, tf.translation, Vec3::splat(0.5)) {
                continue;
            }
            hits.push((e, ent.id));
        }
    }
    if hits.is_empty() {
        return;
    }
    let mut armor = {
        let mut q = world.query::<&mut Player>();
        let Ok(player) = q.get_mut(world, player_e) else {
            return;
        };
        player.armor
    };
    let mut status: Option<String> = None;
    for (e, id) in hits {
        if !heal_allowed(armor) {
            status = Some("Armor full".to_string());
            continue;
        }
        armor += 1;
        world.despawn(e);
        world.resource_mut::<EntityEntities>().0.remove(&id);
        status = Some(format!("Armor {}", armor));
    }
    {
        let mut q = world.query::<&mut Player>();
        if let Ok(mut player) = q.get_mut(world, player_e) {
            player.armor = armor;
        }
    }
    if let Some(status) = status {
        world.resource_mut::<MakerUi>().set_status(status);
    }
}

/// Speed rings apply their boost without touching any shared cooldown, so they
/// cannot suppress bumpers, teleporters, or cannons.
pub fn touch_speed_rings(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let (pt, he, player_e) = {
        let mut q = world.query_filtered::<(Entity, &PlayerTransform, &Player), With<Player>>();
        let Ok((e, pt, player)) = q.single(world) else {
            return;
        };
        (pt.translation, player.half_extents, e)
    };
    let mut hit: Option<(Entity, bool, f32)> = None;
    {
        let mut q = world.query_filtered::<
            (Entity, &Transform, &SpeedRing, Option<&DroppedItem>),
            (Without<Player>, Without<DropPop>),
        >();
        for (e, tf, ring, dropped) in q.iter(world) {
            if !player_overlaps_volume(pt, he, tf.translation, Vec3::splat(0.6)) {
                continue;
            }
            hit = Some((e, dropped.is_some(), ring.duration));
            break;
        }
    }
    let Some((e, dropped, duration)) = hit else {
        return;
    };
    {
        let mut q = world.query::<&mut Player>();
        if let Ok(mut player) = q.get_mut(world, player_e) {
            player.speed_boost = player.speed_boost.max(duration);
        }
    }

    if dropped {
        world.despawn(e);
    }

    world.resource_mut::<MakerUi>().set_status("Speed boost!");
}

/// Crumble plates trigger on top contact (via `InteractionMemory`) with a
/// short warning delay, then stay gone for the run.
pub fn update_crumble_plates(world: &mut World, dt: f32) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let mut updates: Vec<(Entity, bool, bool, f32)> = Vec::new();
    {
        let mut q = world.query_filtered::<(Entity, &LevelEnt, &CrumblePlate), Without<Player>>();
        let memory = world.resource::<InteractionMemory>();
        for (e, ent, plate) in q.iter(world) {
            if plate.gone {
                continue;
            }

            let mut triggered = plate.triggered;
            let mut timer = plate.timer;
            if !triggered && memory.any_entered_target(ent.id) {
                triggered = true;
                timer = plate.delay;
            }

            let mut gone = false;
            if triggered {
                timer -= dt;
                if timer <= 0.0 {
                    gone = true;
                }
            }
            updates.push((e, gone, triggered, timer));
        }
    }
    for (e, gone, triggered, timer) in updates {
        if let Some(mut plate) = world.get_mut::<CrumblePlate>(e) {
            plate.gone = gone;
            plate.triggered = triggered;
            plate.timer = timer;
        }
    }
}

fn placeholder_y_off(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Glimmer => 0.0,
        EntityKind::LaunchPad => -0.1,
        EntityKind::Seal => -0.55,
        EntityKind::DriftPlate => -0.15,
        EntityKind::Prowler => -0.35,
        EntityKind::TriggerOrb => 0.0,
        EntityKind::RelayGate => -1.0,
        EntityKind::Checkpoint => -0.55,
        EntityKind::Teleporter => -0.15,
        EntityKind::Fan => -0.5,
        EntityKind::Bumper => -0.35,
        EntityKind::Crate => -0.5,
        EntityKind::Key => 0.0,
        EntityKind::LockGate => -0.55,
        EntityKind::HealOrb => 0.0,
        EntityKind::SpeedRing => 0.0,
        EntityKind::CrumblePlate => -0.08,
        EntityKind::Cannon => 0.0,
        EntityKind::OnOffSwitch => -0.15,
        EntityKind::TossCrate => -0.5,
        EntityKind::Sign => -0.1,
        EntityKind::Wedge => 0.0,
    }
}

fn wedge_prism_points() -> Vec<Vec3> {
    vec![
        Vec3::new(-0.5, -0.5, -0.5),
        Vec3::new(0.5, -0.5, -0.5),
        Vec3::new(0.5, 0.5, -0.5),
        Vec3::new(-0.5, -0.5, 0.5),
        Vec3::new(0.5, -0.5, 0.5),
        Vec3::new(0.5, 0.5, 0.5),
    ]
}

fn push_box_rotated(group: &mut MeshGroup, center: Vec3, rotation: Quat, he: Vec3, tint: [f32; 3]) {
    let p = |x: f32, y: f32, z: f32| {
        (center + rotation * Vec3::new(x * he.x, y * he.y, z * he.z)).to_array()
    };
    let n = |v: Vec3| (rotation * v).to_array();
    group.push_quad_lit(
        p(-1.0, 1.0, -1.0),
        p(-1.0, 1.0, 1.0),
        p(1.0, 1.0, 1.0),
        p(1.0, 1.0, -1.0),
        tint,
        n(Vec3::Y),
    );
    group.push_quad_lit(
        p(-1.0, -1.0, -1.0),
        p(1.0, -1.0, -1.0),
        p(1.0, -1.0, 1.0),
        p(-1.0, -1.0, 1.0),
        tint,
        n(Vec3::NEG_Y),
    );
    group.push_quad_lit(
        p(1.0, -1.0, -1.0),
        p(1.0, 1.0, -1.0),
        p(1.0, 1.0, 1.0),
        p(1.0, -1.0, 1.0),
        tint,
        n(Vec3::X),
    );
    group.push_quad_lit(
        p(-1.0, -1.0, -1.0),
        p(-1.0, -1.0, 1.0),
        p(-1.0, 1.0, 1.0),
        p(-1.0, 1.0, -1.0),
        tint,
        n(Vec3::NEG_X),
    );
    group.push_quad_lit(
        p(-1.0, -1.0, 1.0),
        p(1.0, -1.0, 1.0),
        p(1.0, 1.0, 1.0),
        p(-1.0, 1.0, 1.0),
        tint,
        n(Vec3::Z),
    );
    group.push_quad_lit(
        p(-1.0, -1.0, -1.0),
        p(-1.0, 1.0, -1.0),
        p(1.0, 1.0, -1.0),
        p(1.0, -1.0, -1.0),
        tint,
        n(Vec3::NEG_Z),
    );
}

fn push_wedge(group: &mut MeshGroup, center: Vec3, rotation: Quat, tint: [f32; 3]) {
    let pts: Vec<Vec3> = wedge_prism_points()
        .into_iter()
        .map(|p| center + rotation * p)
        .collect();
    let [a, b, c, d, e, f] = [pts[0], pts[1], pts[2], pts[3], pts[4], pts[5]];
    let n = |v: Vec3| (rotation * v).to_array();
    group.push_quad_lit(
        a.to_array(),
        d.to_array(),
        f.to_array(),
        c.to_array(),
        tint,
        n(Vec3::new(-1.0, 1.0, 0.0).normalize()),
    );
    group.push_quad_lit(
        d.to_array(),
        a.to_array(),
        b.to_array(),
        e.to_array(),
        tint,
        n(Vec3::NEG_Y),
    );
    group.push_quad_lit(
        b.to_array(),
        c.to_array(),
        f.to_array(),
        e.to_array(),
        tint,
        n(Vec3::X),
    );
    group.push_tri_lit(
        a.to_array(),
        c.to_array(),
        b.to_array(),
        tint,
        n(Vec3::NEG_Z),
    );
    group.push_tri_lit(e.to_array(), f.to_array(), d.to_array(), tint, n(Vec3::Z));
}

fn push_prop(group: &mut MeshGroup, kind: EntityKind, center: Vec3, rotation: Quat) {
    let tint = srgb_to_linear(kind.color());
    match kind {
        EntityKind::Seal => {
            push_box_rotated(group, center, rotation, Vec3::new(0.5, 1.0, 0.15), tint)
        }
        EntityKind::RelayGate => {
            push_box_rotated(group, center, rotation, Vec3::new(0.5, 1.0, 0.2), tint)
        }
        EntityKind::LockGate => {
            push_box_rotated(group, center, rotation, Vec3::new(0.55, 1.2, 0.3), tint)
        }
        EntityKind::Crate | EntityKind::TossCrate => {
            push_box_rotated(group, center, rotation, Vec3::new(0.4, 0.4, 0.4), tint)
        }
        EntityKind::CrumblePlate => {
            push_box_rotated(group, center, rotation, Vec3::new(0.5, 0.12, 0.5), tint)
        }
        EntityKind::LaunchPad => push_box_rotated(
            group,
            center + Vec3::Y * 0.05,
            rotation,
            Vec3::new(0.45, 0.1, 0.45),
            tint,
        ),
        EntityKind::DriftPlate => {
            push_box_rotated(group, center, rotation, Vec3::new(0.7, 0.12, 0.7), tint)
        }
        EntityKind::Wedge => push_wedge(group, center, rotation, tint),
        _ => push_box_rotated(
            group,
            center + Vec3::Y * (placeholder_y_off(kind) + 0.4),
            rotation,
            Vec3::splat(0.4),
            tint,
        ),
    }
}

pub fn draw_props(world: &World, assets: &mut ModelAssets, frame: &mut Frame3d) {
    let elapsed = world.resource::<SimTime>().elapsed_secs as f32;
    let level = world.resource::<LevelDocument>();
    let mut group = MeshGroup {
        depth_test: true,
        ..MeshGroup::default()
    };
    // Instances batch per model: one group per template group, filled as the
    // matching entities are walked.
    let mut by_template: Vec<(usize, Vec<MeshGroup>)> = Vec::new();
    let mut drew = false;
    for e in world.iter_entities() {
        let Some(tf) = e.get::<Transform>() else {
            continue;
        };
        if e.get::<DriftEndMarker>().is_some() {
            push_box_rotated(
                &mut group,
                tf.translation,
                tf.rotation,
                Vec3::splat(0.28) * tf.scale,
                srgb_to_linear(EntityKind::Glimmer.color()),
            );
            drew = true;
            continue;
        }
        let Some(ent) = e.get::<LevelEnt>() else {
            continue;
        };
        if e.get::<DroppedItem>().is_some() {
            let tint = if e.get::<DropGlimmer>().is_some() {
                srgb_to_linear(EntityKind::Glimmer.color())
            } else {
                srgb_to_linear(ent.kind.color())
            };
            push_box_rotated(
                &mut group,
                tf.translation,
                tf.rotation,
                Vec3::splat(0.4) * tf.scale,
                tint,
            );
            drew = true;
            continue;
        }
        if e.get::<Seal>().is_some_and(|s| s.open)
            || e.get::<RelayGate>().is_some_and(|g| g.open)
            || e.get::<LockGate>().is_some_and(|g| g.open)
            || e.get::<CrumblePlate>().is_some_and(|p| p.gone)
        {
            continue;
        }
        let mut center = tf.translation;
        let mut rotation = tf.rotation;
        let scale = tf.scale;
        if let Some(anim) = e.get::<KitAnim>() {
            if anim.bob > 0.0 {
                center.y += (elapsed * 3.0 + anim.seed).sin() * anim.bob;
            }
            rotation = rotation * Quat::from_rotation_y(anim.spin * elapsed);
        }
        match assets.entity(ent.kind) {
            Some(template) => {
                let key = std::rc::Rc::as_ptr(&template) as *const u8 as usize;
                let slot = match by_template.iter().position(|(k, _)| *k == key) {
                    Some(index) => index,
                    None => {
                        by_template.push((
                            key,
                            template
                                .groups
                                .iter()
                                .map(|_| MeshGroup {
                                    depth_test: true,
                                    ..MeshGroup::default()
                                })
                                .collect(),
                        ));
                        by_template.len() - 1
                    }
                };
                let pose = Mat4::from_scale_rotation_translation(scale, rotation, center);
                let matrix = template.matrix_from(pose);
                // `Kind` / `Link` manifest rows are force-tinted flat, as the
                // original did, so channel colour still reads at a glance.
                let flat = match template.tint {
                    crate::maker::assets::TintMode::Model => None,
                    crate::maker::assets::TintMode::Kind => Some(srgb_to_linear(ent.kind.color())),
                    crate::maker::assets::TintMode::Link => level
                        .entity_by_id(ent.id)
                        .map(|data| srgb_to_linear(link_color(data.link))),
                };
                for (dst, src) in by_template[slot].1.iter_mut().zip(template.groups.iter()) {
                    let from = dst.colors.len();
                    push_instance(dst, src, matrix);
                    if let Some(flat) = flat {
                        tint_instance(dst, from, flat);
                    }
                }
                drew = true;
            }
            None => {
                push_prop(&mut group, ent.kind, center, rotation);
                drew = true;
            }
        }
    }
    if drew {
        frame.push(group);
    }
    for (_, groups) in by_template {
        for group in groups {
            if !group.is_empty() {
                frame.push(group);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::maker::collision::rotated_box_aabb;

    fn e() -> LevelEntityId {
        1
    }

    #[test]
    fn hidden_crumble_plate_is_no_longer_solid() {
        // A live plate is solid; once gone it drops out of the solid table.
        let solids = build_solids(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![(e(), Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, false)],
            vec![],
            vec![],
        );
        assert_eq!(solids.len(), 1);

        let solids = build_solids(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![(e(), Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, true)],
            vec![],
            vec![],
        );
        assert!(solids.is_empty());
    }

    #[test]
    fn rotated_gates_use_rotated_collision() {
        let identity = build_solids(
            vec![],
            vec![],
            vec![(e(), Vec3::ZERO, Quat::IDENTITY, false)],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        assert_eq!(identity[0].shape.half_extents(), Vec3::new(0.55, 1.2, 0.3));

        // A thin gate rotated 90° has its wide and thin axes swapped.
        let rot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let (center, he) = rotated_box_aabb(Vec3::ZERO, Vec3::new(0.55, 1.2, 0.3), rot);
        assert_eq!(center, Vec3::ZERO);
        assert!((he.x - 0.3).abs() < 1e-4);
        assert!((he.z - 0.55).abs() < 1e-4);
        assert!((he.y - 1.2).abs() < 1e-4);

        // The rotated gate covers a different footprint than the unrotated one.
        let (_, he_flat) = rotated_box_aabb(Vec3::ZERO, Vec3::new(0.55, 1.2, 0.3), Quat::IDENTITY);
        assert!((he_flat.x - he.x).abs() > 0.1);
    }

    #[test]
    fn wedges_enter_the_solid_table_as_wedge_shapes() {
        let solids = build_solids(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![(e(), Vec3::new(4.0, 0.5, 2.0), Quat::IDENTITY)],
            vec![],
        );
        assert_eq!(solids.len(), 1);
        assert_eq!(solids[0].shape, SolidShape::Wedge(0.5, 0.5, 0.5));
        assert_eq!(solids[0].center, Vec3::new(4.0, 0.5, 2.0));
    }

    #[test]
    fn launch_pads_enter_the_solid_table_as_thin_boxes() {
        // A pad root at y=0.1 becomes a thin standable box whose top sits at
        // root.y + 0.05 + 0.1 = 0.25, matching PAD_TOP_OFFSET.
        let solids = build_solids(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![(e(), Vec3::new(3.0, 0.1, 5.0), Quat::IDENTITY)],
        );
        assert_eq!(solids.len(), 1);
        assert_eq!(
            solids[0].shape,
            SolidShape::Box(0.45, 0.1, 0.45),
            "pads must be thin solid boxes so the player can stand on them"
        );
        assert_eq!(solids[0].center, Vec3::new(3.0, 0.15, 5.0));
        let top = solids[0].center.y + 0.1;
        assert!((top - 0.25).abs() < 1e-5, "pad top at {top}");
    }

    #[test]
    fn reconcile_then_rebuild_builds_expected_solids() {
        let mut sim = repame_shell::Sim::with_default_step();
        sim.world.insert_resource(MakerMode::Edit);
        sim.world.insert_resource(LevelDocument::default());
        sim.world.insert_resource(EntityEntities::default());
        sim.world.insert_resource(RuntimeSolids::default());
        sim.add_chained_systems((reconcile_entities, rebuild_runtime_solids).chain());
        sim.step(Duration::from_millis(17));

        assert_eq!(sim.world.resource::<EntityEntities>().0.len(), 6);

        let solids = &sim.world.resource::<RuntimeSolids>().solids;
        assert_eq!(solids.len(), 3);
        assert_eq!(solids[0].shape, SolidShape::Box(0.5, 1.0, 0.15));
        assert_eq!(solids[0].center, Vec3::new(0.5, 2.0, -3.5));
        assert_eq!(solids[1].shape, SolidShape::Box(0.45, 0.1, 0.45));
        assert_eq!(solids[1].center, Vec3::new(3.5, 1.15, -2.5));
        assert_eq!(solids[2].shape, SolidShape::Box(0.7, 0.12, 0.7));
        assert_eq!(solids[2].center, Vec3::new(-3.5, 2.15, -1.5));
    }

    #[test]
    fn tick_drift_plates_advances_phase_carry_and_velocity() {
        let mut sim = repame_shell::Sim::with_default_step();
        sim.world.insert_resource(MakerMode::Edit);
        let a = Vec3::new(0.5, 0.15, 0.5);
        let b = Vec3::new(4.5, 0.15, 0.5);
        sim.world.spawn((
            Transform::from_translation(a),
            DriftPlate {
                a,
                b,
                period: 1.0,
                t: 0.0,
                carry: Vec3::ZERO,
            },
            Velocity::zero(),
        ));
        sim.add_chained_systems(tick_drift_plates);
        sim.step(Duration::from_millis(17));

        let dt = 1.0f32 / 60.0;
        let phase = dt;
        let s = phase * phase * (3.0 - 2.0 * phase);
        let want = a.lerp(b, s);
        let carry = want - a;

        let mut q = sim.world.query::<(&Transform, &DriftPlate, &Velocity)>();
        let (tf, drift, vel) = q.single(&sim.world).unwrap();
        assert!(
            (tf.translation - want).length() < 1e-5,
            "{:?}",
            tf.translation
        );
        assert!((drift.carry - carry).length() < 1e-5, "{:?}", drift.carry);
        assert!((drift.t - dt).abs() < 1e-6, "{}", drift.t);
        assert!(
            (vel.linear - carry / dt).length() < 1e-4,
            "{:?}",
            vel.linear
        );
        assert_eq!(vel.angular, Vec3::ZERO);
    }
}
