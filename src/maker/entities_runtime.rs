use std::collections::HashMap;

use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use bevy::{
    animation::RepeatAnimation,
    animation::prelude::{
        AnimationClip, AnimationGraph, AnimationGraphHandle, AnimationNodeIndex, AnimationPlayer,
    },
    gltf::Gltf,
};

use game_utils_bevy::juice::Juice;
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};

use super::MakerCleanup;
use super::collision::{is_solid, rotated_box_aabb};
use super::entity_data::{
    ContainedItem, EntityDataExt, EntityKind, EntityKindColor, LevelEntityId, link_color,
};
use super::interaction::{
    InteractionKey, InteractionMemory, MAX_FAN_FORCE, cap_fan_force, gateway_blocked, heal_allowed,
    player_overlaps_volume,
};
use super::level::LevelDocument;
use super::mode::MakerMode;
use super::player::{ActionState, MoveState, Player};
use super::track::{TrackDataExt, TrackId};
use super::ui_bridge::MakerUi;
use bevy_rapier3d::prelude::{Collider, RigidBody, Sensor, Velocity};

#[derive(Resource, Default)]
pub struct EntityEntities(pub HashMap<LevelEntityId, Entity>);

#[derive(Component)]
pub struct LevelEnt {
    pub id: LevelEntityId,
    pub kind: EntityKind,
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
pub struct DriftPlate {
    pub a: Vec3,
    pub b: Vec3,
    pub period: f32,
    pub t: f32,
    pub carry: Vec3,
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
pub fn spawn_drops(
    commands: &mut Commands,
    assets: &EntityAssets,
    counter: &mut DropIdCounter,
    origin: Vec3,
    contents: &Contents,
) {
    let items: Vec<(EntityKind, u32)> = match contents.item {
        ContainedItem::None => return,
        ContainedItem::Glimmers(n) => (0..n).map(|_| (EntityKind::Glimmer, 0)).collect(),
        ContainedItem::Key => vec![(EntityKind::Key, contents.link)],
        ContainedItem::HealOrb => vec![(EntityKind::HealOrb, 0)],
        ContainedItem::SpeedRing => vec![(EntityKind::SpeedRing, 0)],
    };

    let count = items.len().max(1) as f32;

    for (i, (kind, link)) in items.into_iter().enumerate() {
        counter.0 += 1;
        let id = DROP_ID_BASE + counter.0;

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

        let mut ecmds = commands.spawn((
            Transform::from_translation(origin + Vec3::Y * 0.4).with_scale(Vec3::splat(scale)),
            Mesh3d(assets.marker_mesh.clone()),
            MeshMaterial3d(assets.mats[&kind].clone()),
            LevelEnt { id, kind },
            DroppedItem,
            DropPop {
                vel,
                rest_y: origin.y + 0.4,
            },
            MakerCleanup,
        ));

        match kind {
            EntityKind::Glimmer => {
                ecmds.insert(DropGlimmer);
            }
            EntityKind::Key => {
                ecmds.insert(KeyPickup {
                    link: link.clamp(1, 9),
                });
            }
            EntityKind::HealOrb => {
                ecmds.insert(HealOrb);
            }
            EntityKind::SpeedRing => {
                ecmds.insert(SpeedRing { duration: 2.5 });
            }
            _ => {}
        }

        ecmds.insert(Sensor);
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

/// A runtime dynamic solid (gate, seal, crate, crumble plate) used for
/// collision. `rotation` keeps rotated visuals physically aligned.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeSolid {
    #[allow(dead_code)] // reserves future per-owner lookups
    pub owner: Entity,
    pub center: Vec3,
    pub half_extents: Vec3,
    pub rotation: Quat,
}

#[derive(Resource, Default)]
pub struct RuntimeSolids {
    pub solids: Vec<RuntimeSolid>,
}

impl RuntimeSolids {
    /// World-space AABB covers for `move_and_collide` / `stand_headroom`.
    pub fn aabbs(&self) -> Vec<(Vec3, Vec3)> {
        self.solids
            .iter()
            .map(|s| rotated_box_aabb(s.center, s.half_extents, s.rotation))
            .collect()
    }
}

#[derive(Resource)]
pub struct EntityAssets {
    pub scenes: HashMap<EntityKind, Handle<WorldAsset>>,
    pub pad_mesh: Handle<Mesh>,
    pub marker_mesh: Handle<Mesh>,
    pub sign_board_mesh: Handle<Mesh>,
    pub mats: HashMap<EntityKind, Handle<StandardMaterial>>,
    pub link_mats: HashMap<u32, Handle<StandardMaterial>>,
}

/// Marks a spawned glTF scene whose meshes need a material attached once they
/// exist (the fork's glTF loader spawns meshes without `MeshMaterial3d`).
/// Tinted kinds get a flat color (keeps the link-channel / collectible color
/// language); everything else gets the model's albedo texture material.
#[derive(Component)]
pub struct ModelMaterial(pub Handle<StandardMaterial>);

/// Requests that an animated model play named clips (looped) once its
/// `AnimationPlayer` spawns inside the async-loaded scene.
#[derive(Component)]
pub struct ModelAnim {
    /// Key into the [`ClipLibrary`] (the model file).
    pub source: &'static str,
    pub idle: &'static str,
    pub run: Option<&'static str>,
    pub air: Option<&'static str>,
    pub player: Option<Entity>,
    pub started: bool,
    /// clip name -> node index, filled when the graph is built.
    pub nodes: HashMap<&'static str, AnimationNodeIndex>,
    pub state: Option<&'static str>,
}

pub fn setup_entity_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let mut mats = HashMap::new();
    for kind in [
        EntityKind::Glimmer,
        EntityKind::Seal,
        EntityKind::DriftPlate,
        EntityKind::Prowler,
        EntityKind::LaunchPad,
        EntityKind::Checkpoint,
        EntityKind::Teleporter,
        EntityKind::Fan,
        EntityKind::Bumper,
        EntityKind::Crate,
        EntityKind::Key,
        EntityKind::LockGate,
        EntityKind::HealOrb,
        EntityKind::SpeedRing,
        EntityKind::CrumblePlate,
        EntityKind::Cannon,
        EntityKind::OnOffSwitch,
        EntityKind::TossCrate,
        EntityKind::Sign,
    ] {
        let mut m = StandardMaterial::from_color(kind.color());
        m.perceptual_roughness = 0.6;
        m.metallic = 0.2;
        if kind == EntityKind::Glimmer {
            m.emissive = LinearRgba::from(kind.color()) * 4.0;
        }
        mats.insert(kind, materials.add(m));
    }

    let mut link_mats = HashMap::new();
    for ch in 0..=9 {
        let mut m = StandardMaterial::from_color(link_color(ch));
        m.perceptual_roughness = 0.5;
        m.metallic = 0.1;
        m.emissive = LinearRgba::from(link_color(ch)) * 3.0;
        link_mats.insert(ch, materials.add(m));
    }

    let mut scenes = HashMap::new();
    scenes.insert(
        EntityKind::Glimmer,
        asset_server.load("models/pack/Glimmer.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::Seal,
        asset_server.load("models/pack/Seal.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::DriftPlate,
        asset_server.load("models/pack/DriftPlate.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::Prowler,
        asset_server.load("models/pack/Prowler.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::TriggerOrb,
        asset_server.load("models/pack/TriggerOrb.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::RelayGate,
        asset_server.load("models/pack/RelayGate.gltf#Scene0"),
    );

    let pad_mesh = meshes.add(Cylinder::new(0.45, 0.15));
    let marker_mesh = meshes.add(Sphere::new(0.28).mesh().ico(3).unwrap());
    let sign_board_mesh = meshes.add(Cuboid::new(0.9, 0.45, 0.08));

    commands.insert_resource(EntityAssets {
        scenes,
        pad_mesh,
        marker_mesh,
        sign_board_mesh,
        mats,
        link_mats,
    });
}

/// Gameplay position of an entity's root transform (kept identical to the
/// pre-model values so hitboxes and proximity checks are unchanged).
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

/// Per-kind visual config: (scene, material, scene scale, child y-offset).
/// The root transform keeps its current gameplay position; the glTF model is a
/// child so gameplay hitboxes and transforms stay untouched.
fn visual_for(
    kind: EntityKind,
    link: u32,
    assets: &EntityAssets,
) -> Option<(Handle<WorldAsset>, Handle<StandardMaterial>, f32, f32)> {
    let (scale, y_off) = match kind {
        EntityKind::Glimmer => (0.11, -0.20),
        EntityKind::Seal => (0.5, -1.0),
        EntityKind::DriftPlate => (0.8, -0.18),
        EntityKind::Prowler => (0.34, -0.4),
        EntityKind::TriggerOrb => (0.8, -1.0),
        EntityKind::RelayGate => (0.5, -1.0),
        EntityKind::LaunchPad
        | EntityKind::Checkpoint
        | EntityKind::Teleporter
        | EntityKind::Fan
        | EntityKind::Bumper
        | EntityKind::Crate
        | EntityKind::Key
        | EntityKind::LockGate
        | EntityKind::HealOrb
        | EntityKind::SpeedRing
        | EntityKind::CrumblePlate
        | EntityKind::Cannon
        | EntityKind::OnOffSwitch
        | EntityKind::TossCrate
        | EntityKind::Sign => return None,
    };
    let scene = assets.scenes[&kind].clone();
    let material = match kind {
        EntityKind::Glimmer => assets.mats[&kind].clone(),
        EntityKind::TriggerOrb | EntityKind::RelayGate => assets.link_mats[&link.min(9)].clone(),
        // Pack models ship their own multi-material colors (auto-attached by
        // the fork's glTF handler); this is only an inert fallback.
        _ => assets.mats[&kind].clone(),
    };
    Some((scene, material, scale, y_off))
}

/// glTF scenes instantiate asynchronously (a frame after the WorldAssetRoot is
/// spawned). Mesh nodes get their materials from the fork's `bevy_pbr` glTF
/// extension handler, but this runs every frame as a fallback so any mesh node
/// that still lacks a `MeshMaterial3d` gets our model material attached.
pub fn apply_model_materials(
    mut commands: Commands,
    roots: Query<(Entity, &ModelMaterial)>,
    children: Query<&Children>,
    mesh_nodes: Query<(), With<Mesh3d>>,
    matted: Query<(), With<MeshMaterial3d<StandardMaterial>>>,
) {
    for (e, mat) in &roots {
        let mut found = false;
        let mut stack: Vec<Entity> = children
            .get(e)
            .map(|c| c.iter().collect())
            .unwrap_or_default();
        while let Some(ce) = stack.pop() {
            if mesh_nodes.contains(ce) && !matted.contains(ce) {
                commands.entity(ce).insert(MeshMaterial3d(mat.0.clone()));
                found = true;
            }
            if let Ok(grand) = children.get(ce) {
                stack.extend(grand.iter());
            }
        }
        if found {
            commands.entity(e).remove::<ModelMaterial>();
        }
    }
}

/// Which animated model (if any) each entity kind maps to, and its clips.
fn anim_for(kind: EntityKind) -> Option<ModelAnim> {
    match kind {
        EntityKind::Prowler => Some(ModelAnim {
            source: "prowler",
            idle: "Idle",
            run: Some("Walk"),
            air: None,
            player: None,
            started: false,
            nodes: HashMap::new(),
            state: None,
        }),
        _ => None,
    }
}

/// Named animation clips per model, resolved from the loaded [`Gltf`] asset.
#[derive(Resource, Default)]
pub struct ClipLibrary {
    pub pending: Vec<(&'static str, Handle<Gltf>)>,
    pub clips: HashMap<&'static str, HashMap<Box<str>, Handle<AnimationClip>>>,
}

/// Kicks off the `Gltf` loads whose `named_animations` we want to resolve.
pub fn init_clip_library(asset_server: Res<AssetServer>, mut lib: ResMut<ClipLibrary>) {
    lib.pending = vec![
        (
            "player",
            asset_server.load::<Gltf>("models/pack/Player.gltf"),
        ),
        (
            "prowler",
            asset_server.load::<Gltf>("models/pack/Prowler.gltf"),
        ),
    ];
}

/// Copies `named_animations` from each loaded `Gltf` into the [`ClipLibrary`].
pub fn collect_clips(mut lib: ResMut<ClipLibrary>, gltfs: Res<Assets<Gltf>>) {
    let ready: Vec<_> = lib.pending.drain(..).collect();
    for (key, handle) in ready {
        if let Some(gltf) = gltfs.get(&handle) {
            lib.clips.entry(key).or_default().extend(
                gltf.named_animations
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        } else {
            lib.pending.push((key, handle));
        }
    }
}

/// Once a model's scene has spawned its `AnimationPlayer`, build an animation
/// graph from its requested clips, attach it, and start the idle clip.
pub fn apply_model_anims(
    mut commands: Commands,
    lib: Res<ClipLibrary>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut roots: Query<(Entity, &mut ModelAnim, &Children)>,
    children: Query<&Children>,
    mut anims: Query<(Entity, &mut AnimationPlayer)>,
) {
    for (_e, mut anim, root_children) in &mut roots {
        let Some(player) = anim.player else {
            let mut stack: Vec<Entity> = root_children.iter().collect();
            while let Some(ce) = stack.pop() {
                if anims.contains(ce) {
                    anim.player = Some(ce);
                    break;
                }
                if let Ok(grand) = children.get(ce) {
                    stack.extend(grand.iter());
                }
            }
            continue;
        };
        if anim.started {
            continue;
        }
        let Some(clips) = lib.clips.get(anim.source) else {
            continue;
        };
        let mut graph = AnimationGraph::new();
        let root = graph.root;
        let mut node_count = 0;
        for name in [Some(anim.idle), anim.run, anim.air].into_iter().flatten() {
            if let Some(handle) = clips.get(name) {
                let node = graph.add_clip(handle.clone(), 1.0, root);
                anim.nodes.insert(name, node);
                node_count += 1;
            }
        }
        if node_count == 0 {
            continue;
        }
        let handle = graphs.add(graph);
        commands.entity(player).insert(AnimationGraphHandle(handle));
        if let Ok((_, mut p)) = anims.get_mut(player)
            && let Some(node) = anim.nodes.get(anim.idle)
        {
            p.start(*node).set_repeat(RepeatAnimation::Forever);
            anim.state = Some(anim.idle);
        }
        anim.started = true;
    }
}

/// Switches looped clips at runtime: the player picks Idle/Run/Air from its
/// velocity + ground state and faces its wish/move direction; the prowler
/// walks while the game is in Play (its facing is driven by `move_prowlers`).
pub fn tick_model_anims(
    mode: Res<MakerMode>,
    players: Query<(&Player, Option<&MoveState>, &Children)>,
    level_ents: Query<(&LevelEnt, &Children)>,
    mut anims: Query<&mut AnimationPlayer>,
    mut model_anims: Query<(&mut ModelAnim, &mut Transform)>,
) {
    let playing = *mode == MakerMode::Play;
    for (player, move_state, children) in &players {
        let horizontal = player.velocity.xz().length();
        let action = move_state.map(|m| m.action).unwrap_or(ActionState::Run);
        let airborne = matches!(
            action,
            ActionState::Air | ActionState::Slam | ActionState::Launch
        ) || !player.on_ground;
        // Prefer stick/wish so reverse and strafes face the input immediately
        // (velocity lags during turnaround and caused moonwalk + "wrong face").
        let wish = move_state.map(|m| m.wish_dir.xz()).unwrap_or(Vec2::ZERO);
        let vel = player.velocity.xz();
        let face = if wish.length_squared() > 1e-4 {
            wish
        } else if vel.length_squared() > 0.01 {
            vel
        } else {
            Vec2::ZERO
        };
        for child in children.iter() {
            let Ok((mut anim, mut tf)) = model_anims.get_mut(child) else {
                continue;
            };
            if face.length_squared() > 1e-6 {
                let d = face.normalize();
                // Model forward = local -Z (pack convention, matches Prowler):
                // Quat::from_rotation_y(yaw) * NEG_Z == (d.x, 0, d.y).
                tf.rotation = Quat::from_rotation_y((-d.x).atan2(-d.y));
            }
            let target = if matches!(action, ActionState::Swim) {
                if horizontal > 0.6 {
                    anim.run.unwrap_or(anim.idle)
                } else {
                    anim.idle
                }
            } else if airborne && let Some(air) = anim.air {
                air
            } else if !airborne
                && (horizontal > 1.0 || wish.length_squared() > 0.25)
                && let Some(run) = anim.run
            {
                run
            } else {
                anim.idle
            };
            play_if_needed(&mut anim, target, &mut anims);
        }
    }
    for (ent, children) in &level_ents {
        if ent.kind != EntityKind::Prowler {
            continue;
        }
        for child in children.iter() {
            let Ok((mut anim, _tf)) = model_anims.get_mut(child) else {
                continue;
            };
            let target = if playing && let Some(run) = anim.run {
                run
            } else {
                anim.idle
            };
            play_if_needed(&mut anim, target, &mut anims);
        }
    }
}

fn play_if_needed(
    anim: &mut ModelAnim,
    target: &'static str,
    anims: &mut Query<&mut AnimationPlayer>,
) {
    if anim.state == Some(target) {
        return;
    }
    let (Some(pent), Some(node)) = (anim.player, anim.nodes.get(target).copied()) else {
        return;
    };
    if let Ok(mut p) = anims.get_mut(pent) {
        p.start(node).set_repeat(RepeatAnimation::Forever);
        anim.state = Some(target);
    }
}

pub fn reconcile_entities(
    mut commands: Commands,
    mut level: ResMut<LevelDocument>,
    assets: Option<Res<EntityAssets>>,
    mut map: ResMut<EntityEntities>,
    mode: Res<MakerMode>,
) {
    let Some(assets) = assets else {
        return;
    };
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

        let eid = if let Some((scene, material, scale, y_off)) =
            visual_for(data.kind, data.link, &assets)
        {
            let root = commands
                .spawn((
                    tf,
                    LevelEnt {
                        id: data.id,
                        kind: data.kind,
                    },
                    MakerCleanup,
                ))
                .id();
            commands.entity(root).with_children(|p| {
                let mut vis = p.spawn((
                    WorldAssetRoot(scene),
                    MakerCleanup,
                    Visibility::default(),
                    ModelMaterial(material),
                    Transform::from_translation(Vec3::Y * y_off).with_scale(Vec3::splat(scale)),
                ));
                if let Some(anim) = anim_for(data.kind) {
                    vis.insert(anim);
                }
            });
            root
        } else {
            match data.kind {
                EntityKind::LaunchPad => commands
                    .spawn((
                        tf,
                        Mesh3d(assets.pad_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::LaunchPad].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::Checkpoint => commands
                    .spawn((
                        tf.with_scale(Vec3::splat(0.55)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::Checkpoint].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::Teleporter => commands
                    .spawn((
                        tf.with_scale(Vec3::new(0.9, 0.15, 0.9)),
                        Mesh3d(assets.pad_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::Teleporter].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::Fan => commands
                    .spawn((
                        tf.with_scale(Vec3::splat(0.5)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::Fan].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::Bumper => commands
                    .spawn((
                        tf.with_scale(Vec3::splat(0.55)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::Bumper].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::Crate => commands
                    .spawn((
                        tf.with_scale(Vec3::splat(0.9)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::Crate].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::Key => commands
                    .spawn((
                        tf.with_scale(Vec3::splat(0.35)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::Key].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::LockGate => commands
                    .spawn((
                        tf.with_scale(Vec3::new(1.0, 1.2, 0.25)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::LockGate].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::HealOrb => commands
                    .spawn((
                        tf.with_scale(Vec3::splat(0.35)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::HealOrb].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::SpeedRing => commands
                    .spawn((
                        tf.with_scale(Vec3::splat(0.7)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::SpeedRing].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::CrumblePlate => commands
                    .spawn((
                        tf.with_scale(Vec3::new(1.0, 0.12, 1.0)),
                        Mesh3d(assets.pad_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::CrumblePlate].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::Cannon => commands
                    .spawn((
                        tf.with_scale(Vec3::splat(0.5)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::Cannon].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::OnOffSwitch => commands
                    .spawn((
                        tf.with_scale(Vec3::new(0.6, 0.18, 0.6)),
                        Mesh3d(assets.pad_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::OnOffSwitch].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::TossCrate => commands
                    .spawn((
                        tf.with_scale(Vec3::splat(0.8)),
                        Mesh3d(assets.marker_mesh.clone()),
                        MeshMaterial3d(assets.mats[&EntityKind::TossCrate].clone()),
                        LevelEnt {
                            id: data.id,
                            kind: data.kind,
                        },
                        MakerCleanup,
                    ))
                    .id(),

                EntityKind::Sign => {
                    let root = commands
                        .spawn((
                            tf,
                            LevelEnt {
                                id: data.id,
                                kind: data.kind,
                            },
                            MakerCleanup,
                        ))
                        .id();
                    commands.entity(root).with_children(|p| {
                        p.spawn((
                            Transform::from_xyz(0.0, 0.25, 0.0)
                                .with_scale(Vec3::new(0.18, 0.5, 0.18)),
                            Mesh3d(assets.pad_mesh.clone()),
                            MeshMaterial3d(assets.mats[&EntityKind::Sign].clone()),
                            MakerCleanup,
                        ));
                        p.spawn((
                            Transform::from_xyz(0.0, 0.55, 0.0),
                            Mesh3d(assets.sign_board_mesh.clone()),
                            MeshMaterial3d(assets.mats[&EntityKind::Sign].clone()),
                            MakerCleanup,
                        ));
                    });
                    root
                }

                _ => unreachable!("non-visual entity kind without primitive fallback"),
            }
        };

        let ecmds = &mut commands.entity(eid);

        match data.kind {
            EntityKind::Glimmer => {
                ecmds.insert(GlimmerTag);
                ecmds.insert(Sensor);
            }
            EntityKind::LaunchPad => {
                ecmds.insert(LaunchPad {
                    impulse: data.param,
                    yaw_rad: yaw,
                });
                if playing {
                    ecmds.insert((
                        RigidBody::KinematicPositionBased,
                        Collider::cylinder(0.15, 0.45),
                    ));
                }
            }
            EntityKind::Seal => {
                ecmds.insert(Seal {
                    need: data.param.max(1.0) as u32,
                    open: false,
                });
                ecmds.insert(SealSolid);
                if playing {
                    ecmds.insert(Collider::cuboid(0.35, 0.35, 0.35));
                }
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
                    ecmds.insert((
                        RigidBody::KinematicPositionBased,
                        Collider::cuboid(0.7, 0.12, 0.7),
                        Velocity::default(),
                    ));
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
                if playing {
                    ecmds.insert((RigidBody::Fixed, Collider::cuboid(0.5, 1.0, 0.2)));
                }
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
                let dir = Vec3::new(yaw.sin(), 0.0, yaw.cos()).normalize_or_zero();
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
            EntityKind::TossCrate => {
                ecmds.insert(CrateProp {
                    breakable: data.param >= 0.5,
                });
                ecmds.insert(Contents {
                    item: data.contents,
                    link: data.link,
                });
                if playing {
                    ecmds.insert((
                        RigidBody::Dynamic,
                        Collider::cuboid(0.4, 0.4, 0.4),
                        Velocity::default(),
                        super::rapier::Throwable,
                    ));
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
                Mesh3d(assets.marker_mesh.clone()),
                MeshMaterial3d(assets.mats[&EntityKind::Glimmer].clone()),
                Transform::from_translation(b).with_scale(Vec3::splat(0.3)),
                MakerCleanup,
            ));
        }

        map.0.insert(data.id, eid);

        if !playing {
            Juice::pop_in(&mut commands, eid, 0.12);
        }
    }
}

pub fn bob_glimmers(time: Res<Time>, mut q: Query<&mut Transform, With<GlimmerTag>>) {
    let t = time.elapsed_secs();
    for (i, mut tf) in q.iter_mut().enumerate() {
        let bob = (t * 3.0 + i as f32).sin() * 0.04;
        tf.translation.y += bob;
        tf.rotate_y(time.delta_secs() * 2.0);
    }
}

pub fn animate_kit(time: Res<Time>, mut q: Query<(&mut Transform, &KitAnim)>) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    for (mut tf, anim) in &mut q {
        if anim.bob > 0.0 {
            tf.translation.y = anim.base_y + (t * 3.0 + anim.seed).sin() * anim.bob;
        }
        tf.rotate_y(dt * anim.spin);
    }
}

/// Split sign text into dialog lines of at most 30 chars, honoring `\n`
/// (mirrors MB64's 5x30 dialog buffer).
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

pub fn touch_checkpoints(
    mode: Res<MakerMode>,
    mut ui: ResMut<MakerUi>,
    mut player_q: Query<(&Transform, &mut Player)>,
    mut checkpoints: Query<(&LevelEnt, &Transform, &mut Checkpoint)>,
) {
    if *mode != MakerMode::Play {
        return;
    }

    let Ok((pt, mut player)) = player_q.single_mut() else {
        return;
    };

    let mut hit: Option<(LevelEntityId, Vec3)> = None;

    for (ent, tf, cp) in &mut checkpoints {
        if pt.translation.distance(tf.translation) < 1.2 {
            if player.checkpoint_id != Some(ent.id) {
                hit = Some((ent.id, cp.respawn));
            }
            break;
        }
    }

    let Some((new_id, respawn)) = hit else {
        return;
    };

    player.checkpoint_id = Some(new_id);
    player.respawn_point = respawn;

    for (ent, _, mut cp) in &mut checkpoints {
        cp.active = ent.id == new_id;
    }

    ui.set_status("Checkpoint reached!");
}

pub fn collect_glimmers(
    mut commands: Commands,
    mode: Res<MakerMode>,
    _level: Res<LevelDocument>,
    mut ui: ResMut<MakerUi>,
    mut trauma: ResMut<Trauma>,
    player_q: Query<&Transform, With<Player>>,
    glimmers: Query<(Entity, &Transform), With<GlimmerTag>>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let Ok(pt) = player_q.single() else {
        return;
    };
    let mut to_despawn = Vec::new();
    for (e, gt) in &glimmers {
        if pt.translation.distance(gt.translation) < 1.0 {
            to_despawn.push(e);
        }
    }
    let count = to_despawn.len() as u32;
    for e in to_despawn {
        commands.entity(e).despawn();
    }
    if count > 0 {
        let total = ui.glimmers_collected + count;
        ui.glimmers_collected = total;
        ui.set_status(format!("Glimmer x{total}"));
        ScreenEffects::add_trauma(&mut trauma, 0.12 * count as f32);
    }
}

pub fn update_seals(
    mode: Res<MakerMode>,
    ui: Res<MakerUi>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Seal, &mut Visibility, Option<&SealSolid>)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    for (e, mut seal, mut vis, solid) in &mut q {
        let should_open = ui.glimmers_collected >= seal.need;
        if should_open && !seal.open {
            seal.open = true;
            *vis = Visibility::Hidden;
            if solid.is_some() {
                commands.entity(e).remove::<SealSolid>();
            }
        }
    }
}

/// Gates open while (clock - last_pulse) < duration; close crush-safe, waiting
/// for every body (player / crate / prowler) to clear the doorway.
pub fn update_relay_gates(
    mode: Res<MakerMode>,
    link: Res<LinkState>,
    mut commands: Commands,
    player_q: Query<(&Transform, &Player)>,
    prowlers: Query<&Transform, With<Prowler>>,
    crates: Query<&Transform, With<CrateProp>>,
    mut gates: Query<(Entity, &Transform, &mut RelayGate, &mut Visibility), Without<Player>>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let mut bodies: Vec<(Vec3, Vec3)> = Vec::new();
    if let Ok((pt, p)) = player_q.single() {
        bodies.push((pt.translation, p.half_extents));
    }
    for t in &prowlers {
        bodies.push((t.translation, Vec3::splat(0.35)));
    }
    for t in &crates {
        bodies.push((t.translation, Vec3::splat(0.5)));
    }

    for (e, gt, mut gate, mut vis) in &mut gates {
        let powered = gate.channel != 0
            && link
                .pulses
                .get(&gate.channel)
                .is_some_and(|t| link.clock - t < gate.duration);

        if powered && !gate.open {
            gate.open = true;
            gate.want_close = false;
            *vis = Visibility::Hidden;
            commands.entity(e).remove::<GateSolid>();
            commands.entity(e).remove::<Collider>();
        } else if !powered && gate.open {
            // Crush-safe close: wait until nothing is in the doorway.
            if gateway_blocked(
                bodies.iter().copied(),
                gt.translation,
                Vec3::new(0.5, 1.0, 0.2),
            ) {
                gate.want_close = true;
            } else {
                gate.open = false;
                gate.want_close = false;
                *vis = Visibility::Visible;
                commands.entity(e).insert(GateSolid);
                commands.entity(e).insert(Collider::cuboid(0.5, 1.0, 0.2));
            }
        }
    }
}

/// Edit-mode gizmos: dashed lines between same-channel linked entities.
pub fn draw_link_gizmos(mode: Res<MakerMode>, level: Res<LevelDocument>, mut gizmos: Gizmos) {
    if *mode != MakerMode::Edit {
        return;
    }
    let linked: Vec<_> = level
        .data
        .entities
        .iter()
        .filter(|e| e.link != 0 && e.kind.uses_link())
        .collect();
    for a in &linked {
        for b in linked.iter().filter(|g| g.link == a.link && g.id != a.id) {
            let pa = a.cell_i().as_vec3() + Vec3::new(0.5, 1.2, 0.5);
            let pb = b.cell_i().as_vec3() + Vec3::new(0.5, 1.2, 0.5);
            gizmos.line(pa, pb, link_color(a.link));
        }
    }
}

pub fn tick_drift_plates(
    time: Res<Time>,
    _mode: Res<MakerMode>,
    mut plates: Query<(&mut Transform, &mut DriftPlate), Without<TrackFollower>>,
) {
    let dt = time.delta_secs();
    for (mut tf, mut drift) in &mut plates {
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
    }
}

pub fn tick_track_followers(
    time: Res<Time>,
    level: Res<LevelDocument>,
    _mode: Res<MakerMode>,
    mut followers: Query<(&mut Transform, &mut TrackFollower, Option<&mut DriftPlate>)>,
) {
    let dt = time.delta_secs();
    for (mut tf, mut follow, drift) in &mut followers {
        let Some(track) = level.track(follow.track_id) else {
            continue;
        };
        let prev = tf.translation;
        follow.distance += dt * track.speed.max(0.0);
        tf.translation = track.sample(follow.distance);
        // player.rs gates on proximity; carry_player only matters for non-player
        // uses (e.g. hazard prowlers), where carry must not push the player.
        if let Some(mut drift) = drift {
            drift.carry = if follow.carry_player {
                tf.translation - prev
            } else {
                Vec3::ZERO
            };
        }
    }
}

pub fn move_prowlers(
    time: Res<Time>,
    mode: Res<MakerMode>,
    level: Res<LevelDocument>,
    mut q: Query<(&mut Transform, &mut Prowler)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let dt = time.delta_secs();

    for (mut tf, mut p) in &mut q {
        if p.on_track {
            // Track drives translation; we just face travel direction.
            let delta = tf.translation - p.prev;
            let flat = Vec3::new(delta.x, 0.0, delta.z);
            if flat.length_squared() > 1e-6 {
                p.dir = flat.normalize();
                tf.rotation = Quat::from_rotation_y((-p.dir.x).atan2(-p.dir.z));
            }
            p.prev = tf.translation;
            continue;
        }

        // Patrol: step, then flip at walls or ledges (grid-aware).
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

        if wall || ledge {
            p.dir = -p.dir;
        } else {
            tf.translation = next;
        }
        tf.rotation = Quat::from_rotation_y((-p.dir.x).atan2(-p.dir.z));
    }
}

pub fn rebuild_runtime_solids(
    mut solids: ResMut<RuntimeSolids>,
    seals: Query<(Entity, &Transform, &Seal), With<SealSolid>>,
    gates: Query<(Entity, &Transform, &RelayGate), With<GateSolid>>,
    lock_gates: Query<(Entity, &Transform, &LockGate)>,
    crates: Query<(Entity, &Transform, &CrateProp)>,
    plates: Query<(Entity, &Transform, &CrumblePlate)>,
) {
    solids.solids = build_solids(
        seals
            .iter()
            .map(|(e, t, s)| (e, t.translation, t.rotation, s.open))
            .collect(),
        gates
            .iter()
            .map(|(e, t, g)| (e, t.translation, t.rotation, g.open))
            .collect(),
        lock_gates
            .iter()
            .map(|(e, t, l)| (e, t.translation, t.rotation, l.open))
            .collect(),
        crates
            .iter()
            .map(|(e, t, _)| (e, t.translation, t.rotation))
            .collect(),
        plates
            .iter()
            .map(|(e, t, p)| (e, t.translation, t.rotation, p.gone))
            .collect(),
    );
}

/// Pure solid-table builder (shared with tests). Collision state derives from
/// authoritative open/gone flags, never from visibility.
pub fn build_solids(
    seals: Vec<(Entity, Vec3, Quat, bool)>,
    gates: Vec<(Entity, Vec3, Quat, bool)>,
    lock_gates: Vec<(Entity, Vec3, Quat, bool)>,
    crates: Vec<(Entity, Vec3, Quat)>,
    plates: Vec<(Entity, Vec3, Quat, bool)>,
) -> Vec<RuntimeSolid> {
    let mut out = Vec::new();
    for (e, center, rotation, open) in seals {
        if !open {
            out.push(RuntimeSolid {
                owner: e,
                center,
                half_extents: Vec3::new(0.5, 1.0, 0.15),
                rotation,
            });
        }
    }
    for (e, center, rotation, open) in gates {
        if !open {
            out.push(RuntimeSolid {
                owner: e,
                center,
                half_extents: Vec3::new(0.5, 1.0, 0.2),
                rotation,
            });
        }
    }
    for (e, center, rotation, open) in lock_gates {
        if !open {
            out.push(RuntimeSolid {
                owner: e,
                center,
                half_extents: Vec3::new(0.55, 1.2, 0.3),
                rotation,
            });
        }
    }
    for (e, center, rotation) in crates {
        out.push(RuntimeSolid {
            owner: e,
            center,
            half_extents: Vec3::new(0.5, 0.5, 0.5),
            rotation,
        });
    }
    for (e, center, rotation, gone) in plates {
        if !gone {
            out.push(RuntimeSolid {
                owner: e,
                center,
                half_extents: Vec3::new(0.5, 0.12, 0.5),
                rotation,
            });
        }
    }
    out
}

/// Fans accumulate a capped force. Continuous fans apply after forced motion
/// unless a position override (teleport / respawn) won this frame.
pub fn apply_fans(
    time: Res<Time>,
    mode: Res<MakerMode>,
    forced: Res<super::interaction::ForcedMotionRequests>,
    mut player_q: Query<(&Transform, &mut Player)>,
    fans: Query<(&Transform, &Fan), Without<Player>>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    if forced.position_applied {
        return;
    }
    let Ok((pt, mut player)) = player_q.single_mut() else {
        return;
    };
    let dt = time.delta_secs();

    let mut force = Vec3::ZERO;
    for (tf, fan) in &fans {
        // Simple axis-aligned volume in front of the fan.
        let to_p = pt.translation - tf.translation;
        let ahead = to_p.dot(fan.dir);
        if ahead < 0.0 || ahead > 4.0 {
            continue;
        }
        let lateral = (to_p - fan.dir * ahead).length();
        if lateral > 1.4 {
            continue;
        }
        force += fan.dir * fan.strength * dt;
        // slight lift so fans feel useful in 3D platforming
        force.y += fan.strength * 0.15 * dt;
    }
    player.velocity += cap_fan_force(force, MAX_FAN_FORCE);
}

pub fn update_drops(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut DropPop), With<DroppedItem>>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut pop) in &mut q {
        pop.vel.y -= 22.0 * dt;
        tf.translation += pop.vel * dt;
        if pop.vel.y < 0.0 && tf.translation.y <= pop.rest_y {
            tf.translation.y = pop.rest_y;
            commands.entity(e).remove::<DropPop>();
        }
    }
}

pub fn collect_dropped_glimmers(
    mut commands: Commands,
    mode: Res<MakerMode>,
    mut ui: ResMut<MakerUi>,
    player_q: Query<(&Transform, &Player)>,
    drops: Query<(Entity, &Transform), (With<DropGlimmer>, Without<DropPop>)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let Ok((pt, _)) = player_q.single() else {
        return;
    };

    for (e, tf) in &drops {
        if pt.translation.distance(tf.translation) > 1.0 {
            continue;
        }
        commands.entity(e).despawn();
        ui.glimmers_collected += 1;
        ui.score += 100;
        let (c, t) = (ui.glimmers_collected, ui.glimmers_total);
        ui.set_status(format!("Glimmer {c}/{t}"));
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

pub fn collect_keys(
    mut commands: Commands,
    mode: Res<MakerMode>,
    mut ui: ResMut<MakerUi>,
    mut map: ResMut<EntityEntities>,
    mut player_q: Query<(&Transform, &mut Player)>,
    keys: Query<(Entity, &Transform, &LevelEnt, &KeyPickup), (Without<Player>, Without<DropPop>)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let Ok((pt, mut player)) = player_q.single_mut() else {
        return;
    };
    let he = player.half_extents;

    for (e, tf, ent, key) in &keys {
        if !player_overlaps_volume(pt.translation, he, tf.translation, Vec3::splat(0.5)) {
            continue;
        }
        let ch = key.link as usize;
        if ch < player.keys.len() {
            player.keys[ch] = player.keys[ch].saturating_add(1);
        }
        commands.entity(e).despawn();
        map.0.remove(&ent.id);
        ui.set_status(format!("Key (ch {ch})"));
    }
}

/// Lock gates unlock only through the arbitrated explicit use target
/// (`interaction::resolve_use`). This system handles timed closing and re-arms
/// the gate; closing waits until the doorway is clear, so it never consumes a
/// second key merely because a timed gate closed while the player stayed near.
pub fn update_lock_gates(
    time: Res<Time>,
    mode: Res<MakerMode>,
    player_q: Query<(&Transform, &Player)>,
    prowlers: Query<&Transform, With<Prowler>>,
    crates: Query<&Transform, With<CrateProp>>,
    mut gates: Query<(&mut LockGate, &mut Visibility, &Transform), Without<Player>>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let dt = time.delta_secs();

    let mut bodies: Vec<(Vec3, Vec3)> = Vec::new();
    if let Ok((pt, p)) = player_q.single() {
        bodies.push((pt.translation, p.half_extents));
    }
    for t in &prowlers {
        bodies.push((t.translation, Vec3::splat(0.35)));
    }
    for t in &crates {
        bodies.push((t.translation, Vec3::splat(0.5)));
    }

    for (mut gate, mut vis, tf) in &mut gates {
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
        {
            gate.open = false;
            *vis = Visibility::Visible;
        }
    }
}

/// Heal orbs use shape overlap and are only consumed when armor is below the
/// cap. Drop-pop animation still blocks pickup via `Without<DropPop>`.
pub fn collect_heal_orbs(
    mut commands: Commands,
    mode: Res<MakerMode>,
    mut ui: ResMut<MakerUi>,
    mut map: ResMut<EntityEntities>,
    mut player_q: Query<(&Transform, &mut Player)>,
    orbs: Query<
        (Entity, &Transform, &LevelEnt),
        (With<HealOrb>, Without<Player>, Without<DropPop>),
    >,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let Ok((pt, mut player)) = player_q.single_mut() else {
        return;
    };
    let he = player.half_extents;

    for (e, tf, ent) in &orbs {
        if !player_overlaps_volume(pt.translation, he, tf.translation, Vec3::splat(0.5)) {
            continue;
        }
        if !heal_allowed(player.armor) {
            ui.set_status("Armor full");
            continue;
        }
        player.armor += 1;
        commands.entity(e).despawn();
        map.0.remove(&ent.id);
        ui.set_status(format!("Armor {}", player.armor));
    }
}

/// Speed rings apply their boost without touching any shared cooldown, so they
/// cannot suppress bumpers, teleporters, or cannons.
pub fn touch_speed_rings(
    mut commands: Commands,
    mode: Res<MakerMode>,
    mut ui: ResMut<MakerUi>,
    mut player_q: Query<(&Transform, &mut Player)>,
    rings: Query<
        (Entity, &Transform, &SpeedRing, Option<&DroppedItem>),
        (Without<Player>, Without<DropPop>),
    >,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let Ok((pt, mut player)) = player_q.single_mut() else {
        return;
    };
    let he = player.half_extents;

    for (e, tf, ring, dropped) in &rings {
        if !player_overlaps_volume(pt.translation, he, tf.translation, Vec3::splat(0.6)) {
            continue;
        }
        player.speed_boost = player.speed_boost.max(ring.duration);

        if dropped.is_some() {
            commands.entity(e).despawn();
        }

        ui.set_status("Speed boost!");
        break;
    }
}

/// Crumble plates trigger on top contact (via `InteractionMemory`) with a
/// short warning delay, then stay gone for the run.
pub fn update_crumble_plates(
    time: Res<Time>,
    mode: Res<MakerMode>,
    memory: Res<InteractionMemory>,
    mut plates: Query<(&LevelEnt, &mut CrumblePlate, &mut Visibility), Without<Player>>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let dt = time.delta_secs();

    for (ent, mut plate, mut vis) in &mut plates {
        if plate.gone {
            *vis = Visibility::Hidden;
            continue;
        }

        if !plate.triggered && memory.entered(InteractionKey::player(ent.id)) {
            plate.triggered = true;
            plate.timer = plate.delay;
        }

        if plate.triggered {
            plate.timer -= dt;
            if plate.timer <= 0.0 {
                plate.gone = true;
                *vis = Visibility::Hidden;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: Entity = Entity::from_raw(1);

    #[test]
    fn hidden_crumble_plate_is_no_longer_solid() {
        // A live plate is solid; once gone it drops out of the solid table.
        let solids = build_solids(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![(E, Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, false)],
        );
        assert_eq!(solids.len(), 1);

        let solids = build_solids(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![(E, Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, true)],
        );
        assert!(solids.is_empty());
    }

    #[test]
    fn rotated_gates_use_rotated_collision() {
        let identity = build_solids(
            vec![],
            vec![],
            vec![(E, Vec3::ZERO, Quat::IDENTITY, false)],
            vec![],
            vec![],
        );
        assert_eq!(identity[0].half_extents, Vec3::new(0.55, 1.2, 0.3));

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
}
