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
use game_utils_bevy::screen_effects::{FlashWhite, ScreenEffects, Trauma};

use super::MakerCleanup;
use super::collision::is_solid;
use super::entity_data::{EntityDataExt, EntityKind, EntityKindColor, LevelEntityId, link_color};
use super::level::LevelDocument;
use super::mode::MakerMode;
use super::player::{ActionState, JUMP_SPEED, MoveState, Player, spawn_center};
use super::track::{TrackDataExt, TrackId};
use super::ui_bridge::MakerUi;
#[cfg(feature = "physics")]
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
    pub cooldown: f32,
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

/// Last pulse time per channel, in seconds of play-session time.
#[derive(Resource, Default)]
pub struct LinkState {
    pub pulses: std::collections::HashMap<u32, f32>,
    pub clock: f32,
}

#[derive(Resource, Default)]
pub struct RuntimeSolids {
    pub boxes: Vec<(Vec3, Vec3)>,
}

#[derive(Resource)]
pub struct EntityAssets {
    pub scenes: HashMap<EntityKind, Handle<WorldAsset>>,
    pub albedo_mats: HashMap<EntityKind, Handle<StandardMaterial>>,
    pub pad_mesh: Handle<Mesh>,
    pub marker_mesh: Handle<Mesh>,
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
    for kind in [EntityKind::Glimmer, EntityKind::LaunchPad] {
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
        asset_server.load("models/cubeworld/Crystal_Big.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::Seal,
        asset_server.load("models/cubeworld/Door_Closed.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::DriftPlate,
        asset_server.load("models/cubeworld/Cart.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::Prowler,
        asset_server.load("models/cubeworld/Goblin.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::TriggerOrb,
        asset_server.load("models/cubeworld/Button.gltf#Scene0"),
    );
    scenes.insert(
        EntityKind::RelayGate,
        asset_server.load("models/cubeworld/Door_Closed.gltf#Scene0"),
    );

    let pad_mesh = meshes.add(Cylinder::new(0.45, 0.15));
    let marker_mesh = meshes.add(Sphere::new(0.28).mesh().ico(3).unwrap());

    let mut albedo_mats = HashMap::new();
    let mut albedo = |path: &'static str| {
        let texture: Handle<Image> = asset_server.load(path);
        materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            perceptual_roughness: 0.9,
            ..default()
        })
    };
    albedo_mats.insert(
        EntityKind::Seal,
        albedo("models/cubeworld/Door_Closed.gltf#Texture0"),
    );
    albedo_mats.insert(
        EntityKind::DriftPlate,
        albedo("models/cubeworld/Cart.gltf#Texture0"),
    );
    albedo_mats.insert(
        EntityKind::Prowler,
        albedo("models/cubeworld/Goblin.gltf#Texture0"),
    );

    commands.insert_resource(EntityAssets {
        scenes,
        albedo_mats,
        pad_mesh,
        marker_mesh,
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
    }
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
        EntityKind::LaunchPad => return None, // stays a primitive cylinder
    };
    let scene = assets.scenes[&kind].clone();
    let material = match kind {
        EntityKind::Glimmer => assets.mats[&kind].clone(),
        EntityKind::TriggerOrb | EntityKind::RelayGate => assets.link_mats[&link.min(9)].clone(),
        _ => assets.albedo_mats[&kind].clone(),
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
            asset_server.load::<Gltf>("models/cubeworld/Character_Male_2.gltf"),
        ),
        (
            "prowler",
            asset_server.load::<Gltf>("models/cubeworld/Goblin.gltf"),
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
/// velocity + ground state and faces its movement direction; the prowler
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
        let dir = player.velocity.xz();
        let moving = dir.length_squared() > 0.01;
        for child in children.iter() {
            let Ok((mut anim, mut tf)) = model_anims.get_mut(child) else {
                continue;
            };
            if moving {
                let d = dir.normalize();
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
                && horizontal > 1.0
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

    for data in &level.data.entities {
        let world = data.cell_i().as_vec3() + Vec3::new(0.5, 0.0, 0.5);
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
                    ModelMaterial(material),
                    Transform::from_translation(Vec3::Y * y_off).with_scale(Vec3::splat(scale)),
                ));
                if let Some(anim) = anim_for(data.kind) {
                    vis.insert(anim);
                }
            });
            root
        } else {
            commands
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
                .id()
        };

        let ecmds = &mut commands.entity(eid);

        match data.kind {
            EntityKind::Glimmer => {
                ecmds.insert(GlimmerTag);
                #[cfg(feature = "physics")]
                ecmds.insert(Sensor);
            }
            EntityKind::LaunchPad => {
                ecmds.insert(LaunchPad {
                    impulse: data.param,
                    yaw_rad: yaw,
                    cooldown: 0.0,
                });
                #[cfg(feature = "physics")]
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
                #[cfg(feature = "physics")]
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
                #[cfg(feature = "physics")]
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
            }
            EntityKind::TriggerOrb => {
                ecmds.insert(TriggerOrb {
                    channel: data.link,
                    cooldown: data.param.max(0.2),
                    timer: 0.0,
                });
                #[cfg(feature = "physics")]
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
                #[cfg(feature = "physics")]
                if playing {
                    ecmds.insert((RigidBody::Fixed, Collider::cuboid(0.5, 1.0, 0.2)));
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

pub fn tick_launch_pads_cooldown(time: Res<Time>, mut pads: Query<&mut LaunchPad>) {
    let dt = time.delta_secs();
    for mut pad in &mut pads {
        pad.cooldown = (pad.cooldown - dt).max(0.0);
    }
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

/// Player touches an orb -> pulse its channel.
pub fn trigger_orbs(
    time: Res<Time>,
    mode: Res<MakerMode>,
    mut link: ResMut<LinkState>,
    mut ui: ResMut<MakerUi>,
    mut trauma: ResMut<Trauma>,
    mut commands: Commands,
    player_q: Query<&Transform, With<Player>>,
    mut orbs: Query<(Entity, &Transform, &mut TriggerOrb)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    link.clock += time.delta_secs();
    let Ok(pt) = player_q.single() else {
        return;
    };

    for (e, ot, mut orb) in &mut orbs {
        orb.timer = (orb.timer - time.delta_secs()).max(0.0);
        if orb.channel == 0 || orb.timer > 0.0 {
            continue;
        }
        if pt.translation.distance(ot.translation) < 1.0 {
            orb.timer = orb.cooldown;
            let t = link.clock;
            link.pulses.insert(orb.channel, t);
            Juice::pop_in(&mut commands, e, 0.15);
            ScreenEffects::add_trauma(&mut trauma, 0.1);
            ui.set_status(format!("Channel {} triggered!", orb.channel));
        }
    }
}

/// Gates open while (clock - last_pulse) < duration; close crush-safe.
pub fn update_relay_gates(
    mode: Res<MakerMode>,
    link: Res<LinkState>,
    mut commands: Commands,
    player_q: Query<(&Transform, &Player)>,
    mut gates: Query<(Entity, &Transform, &mut RelayGate, &mut Visibility)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let player = player_q.single().ok();

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
            #[cfg(feature = "physics")]
            commands.entity(e).remove::<Collider>();
        } else if !powered && gate.open {
            // Crush-safe close: wait until the player isn't in the doorway.
            let blocked = player.is_some_and(|(pt, p)| {
                let d = (pt.translation - gt.translation).abs();
                d.x < p.half_extents.x + 0.5
                    && d.y < p.half_extents.y + 1.0
                    && d.z < p.half_extents.z + 0.2
            });
            if blocked {
                gate.want_close = true;
            } else {
                gate.open = false;
                gate.want_close = false;
                *vis = Visibility::Visible;
                commands.entity(e).insert(GateSolid);
                #[cfg(feature = "physics")]
                commands.entity(e).insert(Collider::cuboid(0.5, 1.0, 0.2));
            }
        }
    }
}

/// Edit-mode gizmos: dashed lines between same-channel orbs and gates.
pub fn draw_link_gizmos(mode: Res<MakerMode>, level: Res<LevelDocument>, mut gizmos: Gizmos) {
    if *mode != MakerMode::Edit {
        return;
    }
    let orbs: Vec<_> = level
        .data
        .entities
        .iter()
        .filter(|e| e.kind == EntityKind::TriggerOrb && e.link != 0)
        .collect();
    let gates: Vec<_> = level
        .data
        .entities
        .iter()
        .filter(|e| e.kind == EntityKind::RelayGate && e.link != 0)
        .collect();
    for orb in &orbs {
        for gate in gates.iter().filter(|g| g.link == orb.link) {
            let a = orb.cell_i().as_vec3() + Vec3::new(0.5, 1.2, 0.5);
            let b = gate.cell_i().as_vec3() + Vec3::new(0.5, 1.2, 0.5);
            gizmos.line(a, b, link_color(orb.link));
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

pub fn prowler_touch(
    mut commands: Commands,
    mode: Res<MakerMode>,
    level: Res<LevelDocument>,
    mut ui: ResMut<MakerUi>,
    mut trauma: ResMut<Trauma>,
    mut flash: ResMut<FlashWhite>,
    mut map: ResMut<EntityEntities>,
    mut player_q: Query<(Entity, &mut Transform, &mut Player), Without<Prowler>>,
    prowlers: Query<(Entity, &Transform, &LevelEnt), (With<Prowler>, Without<Player>)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let Ok((player_e, mut pt, mut player)) = player_q.single_mut() else {
        return;
    };

    let he = player.half_extents;
    let ph = Vec3::splat(0.35);

    for (prow_e, prow_tf, ent) in &prowlers {
        let d = (pt.translation - prow_tf.translation).abs();
        let overlap = d.x < he.x + ph.x && d.y < he.y + ph.y && d.z < he.z + ph.z;
        if !overlap {
            continue;
        }

        let player_bottom = pt.translation.y - he.y;
        let is_stomp = player.velocity.y < -0.5 && player_bottom > prow_tf.translation.y - 0.05;

        if is_stomp {
            commands.entity(prow_e).despawn();
            map.0.remove(&ent.id);

            player.velocity.y = JUMP_SPEED * 0.8;
            player.on_ground = false;
            player.coyote = 0.0;
            Juice::squash_stretch(&mut commands, player_e, Vec2::new(1.3, 0.7), 0.12);
            ScreenEffects::add_trauma(&mut trauma, 0.18);
            ui.score += 200;
            let total = ui.score;
            ui.set_status(format!("Prowler defeated! +{total}"));
        } else {
            pt.translation = spawn_center(&level);
            player.velocity = Vec3::ZERO;
            player.on_ground = false;
            ui.deaths += 1;
            ScreenEffects::add_trauma(&mut trauma, 0.35);
            ScreenEffects::flash_white(&mut flash, 0.15);
            ui.set_status("Ouch!");
            break;
        }
    }
}

pub fn rebuild_runtime_solids(
    mut solids: ResMut<RuntimeSolids>,
    seals: Query<(&Transform, &Seal), With<SealSolid>>,
    gates: Query<(&Transform, &RelayGate), With<GateSolid>>,
) {
    solids.boxes.clear();
    for (tf, seal) in &seals {
        if !seal.open {
            solids
                .boxes
                .push((tf.translation, Vec3::new(0.5, 1.0, 0.15)));
        }
    }
    for (tf, gate) in &gates {
        if !gate.open {
            solids
                .boxes
                .push((tf.translation, Vec3::new(0.5, 1.0, 0.2)));
        }
    }
}
