use std::collections::HashMap;

use bevy::prelude::*;

use game_utils_bevy::juice::Juice;
use game_utils_bevy::screen_effects::{FlashWhite, ScreenEffects, Trauma};

use super::MakerCleanup;
use super::collision::is_solid;
use super::entity_data::{EntityKind, LevelEntityId};
use super::level::LevelDocument;
use super::mode::MakerMode;
use super::player::{JUMP_SPEED, Player, spawn_center};
use super::track::TrackId;
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

#[derive(Resource, Default)]
pub struct RuntimeSolids {
    pub boxes: Vec<(Vec3, Vec3)>,
}

#[derive(Resource)]
pub struct EntityAssets {
    pub glimmer: Handle<Mesh>,
    pub pad: Handle<Mesh>,
    pub seal: Handle<Mesh>,
    pub drift: Handle<Mesh>,
    pub prowler: Handle<Mesh>,
    pub mats: HashMap<EntityKind, Handle<StandardMaterial>>,
}

pub fn setup_entity_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mats = HashMap::new();
    for kind in [
        EntityKind::Glimmer,
        EntityKind::LaunchPad,
        EntityKind::Seal,
        EntityKind::DriftPlate,
        EntityKind::Prowler,
    ] {
        let mut m = StandardMaterial::from_color(kind.color());
        m.perceptual_roughness = 0.6;
        m.metallic = 0.2;
        if kind == EntityKind::Glimmer {
            m.emissive = LinearRgba::from(kind.color()) * 4.0;
        }
        mats.insert(kind, materials.add(m));
    }

    let glimmer = meshes.add(Sphere::new(0.28).mesh().ico(3).unwrap());
    let pad = meshes.add(Cylinder::new(0.45, 0.15));
    let seal = meshes.add(Cuboid::new(1.0, 2.0, 0.25));
    let drift = meshes.add(Cuboid::new(1.4, 0.25, 1.4));
    let prowler = meshes.add(Cuboid::new(0.7, 0.7, 0.7));

    commands.insert_resource(EntityAssets {
        glimmer,
        pad,
        seal,
        drift,
        prowler,
        mats,
    });
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

        let (mesh, y_off, _scale) = match data.kind {
            EntityKind::Glimmer => (assets.glimmer.clone(), 1.0, Vec3::ONE),
            EntityKind::LaunchPad => (assets.pad.clone(), 0.1, Vec3::ONE),
            EntityKind::Seal => (assets.seal.clone(), 1.0, Vec3::ONE),
            EntityKind::DriftPlate => (assets.drift.clone(), 0.15, Vec3::ONE),
            EntityKind::Prowler => (assets.prowler.clone(), 0.4, Vec3::ONE),
        };

        let mut tf = Transform::from_translation(world + Vec3::Y * y_off).with_rotation(rot);
        let mut track_distance = 0.0;
        if let Some(track_id) = data.track
            && let Some((d, nearest, _)) = level.track(track_id).and_then(|t| t.nearest(world))
        {
            track_distance = d;
            tf.translation = nearest;
        }

        let eid = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(assets.mats[&data.kind].clone()),
                tf,
                LevelEnt {
                    id: data.id,
                    kind: data.kind,
                },
                MakerCleanup,
            ))
            .id();

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
                Mesh3d(assets.glimmer.clone()),
                MeshMaterial3d(assets.mats[&EntityKind::DriftPlate].clone()),
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
    drifts: Query<&Transform, (With<DriftPlate>, Without<SealSolid>)>,
) {
    solids.boxes.clear();
    for (tf, seal) in &seals {
        if !seal.open {
            solids
                .boxes
                .push((tf.translation, Vec3::new(0.5, 1.0, 0.15)));
        }
    }
    for tf in &drifts {
        solids
            .boxes
            .push((tf.translation, Vec3::new(0.7, 0.12, 0.7)));
    }
}
