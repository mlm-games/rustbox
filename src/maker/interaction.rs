use std::collections::HashSet;

use bevy_ecs::prelude::{Entity, Mut, Resource, With, Without, World};
use glam::{Quat, Vec3};
use repame_rapier3d::VelocityRequest3d;

use super::Paused;
use super::block::BlockKind;
use super::camera::CameraRig;
use super::collision::{aabb_hits_solid, overlaps_kind, rotated_box_aabb};
use super::entities_runtime::{
    Bumper, Cannon, Contents, CrateProp, CrumblePlate, EntityEntities, Held, LaunchPad, LevelEnt,
    LinkState, LockGate, OnOffSwitch, Prowler, Sign, Teleporter, Throwable, Transform, TriggerOrb,
    spawn_drops, wrap_sign_text,
};
use super::entity_data::LevelEntityId;
use super::level::LevelDocument;
use super::mode::MakerMode;
use super::player::{
    ActionState, JUMP_SPEED, MoveState, PlayIntent, Player, PlayerMoveMode, PlayerTransform,
    Trauma, respawn_player,
};
use super::props::{RuntimeSolids, Velocity};
use super::win::MakerUi;

/// Sentinel actor id used for the player in per-target contact tracking.
pub const PLAYER_ACTOR: u32 = u32::MAX;
/// Sentinel target id for requests that do not name a real entity.
pub const NO_TARGET: LevelEntityId = 0;
/// Sentinel target id marking a manual (R) respawn request.
pub const RESET_TARGET: LevelEntityId = u32::MAX;

/// How an interaction reacts to contact with an actor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActivationMode {
    /// Fires once when the actor enters the volume; re-arms on exit.
    TouchEnter,
    /// Fires once when the actor lands on the target's top surface.
    TopContact,
    /// Fires on an explicit interact press (arbitrated by `UseSelection`).
    UsePressed,
    /// Applies a continuous effect while overlapping.
    Continuous,
}

/// The family of gameplay objects an interaction target belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InteractionKind {
    Switch,
    Trigger,
    Sign,
    Pickup,
    Checkpoint,
    Teleport,
    Launch,
    Bump,
    Cannon,
    Damage,
    Break,
    Unlock,
}

/// A unique per-actor / per-target contact key. `actor` is the Bevy entity
/// index (or `PLAYER_ACTOR`), `target` the authored `LevelEntityId`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct InteractionKey {
    pub actor: u32,
    pub target: LevelEntityId,
}

impl InteractionKey {
    pub fn player(target: LevelEntityId) -> Self {
        Self {
            actor: PLAYER_ACTOR,
            target,
        }
    }
}

/// Per-frame contact latch. A target that was not touching last frame but is
/// touching now reports `entered()` for exactly one frame; re-arming requires
/// the actor to leave the volume.
#[derive(Resource, Default)]
pub struct InteractionMemory {
    pub previous_contacts: HashSet<InteractionKey>,
    pub current_contacts: HashSet<InteractionKey>,
    /// Destination teleporter holding the post-arrival exit lock, if any.
    pub teleport_exit_lock: Option<InteractionKey>,
}

impl InteractionMemory {
    /// True exactly on the frame the actor started touching the target.
    pub fn entered(&self, key: InteractionKey) -> bool {
        self.current_contacts.contains(&key) && !self.previous_contacts.contains(&key)
    }

    /// True if *any* actor entered `target` this frame (player or throwable).
    pub fn any_entered_target(&self, target: LevelEntityId) -> bool {
        self.current_contacts
            .iter()
            .any(|k| k.target == target && !self.previous_contacts.contains(k))
    }

    /// Roll the frame. Call once at the start of the Detect phase.
    pub fn begin_frame(&mut self) {
        self.previous_contacts.clear();
        std::mem::swap(&mut self.previous_contacts, &mut self.current_contacts);
        self.current_contacts.clear();
    }

    pub fn touch(&mut self, key: InteractionKey) {
        self.current_contacts.insert(key);
    }

    pub fn reset(&mut self) {
        self.previous_contacts.clear();
        self.current_contacts.clear();
        self.teleport_exit_lock = None;
    }
}

/// Priority tiers for forced-motion requests (higher wins).
pub const PRIORITY_RESPAWN: u8 = 100;
pub const PRIORITY_TELEPORT: u8 = 90;
pub const PRIORITY_CANNON: u8 = 80;
pub const PRIORITY_LAUNCH: u8 = 70;
pub const PRIORITY_BUMPER: u8 = 60;

/// A single-frame request to override the actor's motion. Only the
/// highest-priority request is honored per frame; continuous forces (fans)
/// apply afterward unless a position override won.
#[derive(Clone, Copy, Debug)]
pub struct ForcedMotion {
    /// The interaction that produced this request (identifies the respawn
    /// reason for full-respawn requests).
    pub source: InteractionKey,
    pub priority: u8,
    pub position: Option<Vec3>,
    pub velocity: Option<Vec3>,
    pub control_lock: f32,
    /// Full respawn (honors checkpoints). Implies a position + velocity reset.
    pub respawn: bool,
}

#[derive(Resource, Default)]
pub struct ForcedMotionRequests {
    pub requests: Vec<ForcedMotion>,
    /// Set when a position-overriding request (teleport / respawn) applied.
    pub position_applied: bool,
}

impl ForcedMotionRequests {
    pub fn clear(&mut self) {
        self.requests.clear();
        self.position_applied = false;
    }

    pub fn push(&mut self, motion: ForcedMotion) {
        self.requests.push(motion);
    }

    /// The highest-priority request this frame, if any.
    pub fn winner(&self) -> Option<&ForcedMotion> {
        self.requests.iter().max_by_key(|r| r.priority)
    }
}

/// The attack that produced a damage event. `None` = plain contact damage.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AttackKind {
    Stomp,
    Slam,
    ThrownImpact,
}

/// One damage/break event. Deduplicated per (target, attack) each frame so a
/// single collision cannot kill twice or drop twice.
#[derive(Clone, Copy, Debug)]
pub struct DamageRequest {
    pub target: Entity,
    /// The attacking actor; the player only bounces off a defeated prowler
    /// when it was the player's own stomp that landed the kill.
    pub source: Entity,
    /// Armor consumed per hit (reserved for multi-point attacks).
    pub amount: u8,
    pub attack: Option<AttackKind>,
}

#[derive(Resource, Default)]
pub struct DamageRequests {
    pub requests: Vec<DamageRequest>,
}

impl DamageRequests {
    pub fn clear(&mut self) {
        self.requests.clear();
    }

    pub fn push(&mut self, request: DamageRequest) {
        self.requests.push(request);
    }

    pub fn unique(&self) -> Vec<DamageRequest> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for r in &self.requests {
            if seen.insert(r.target) {
                out.push(*r);
            }
        }
        out
    }
}

/// Priority tiers for explicit-use arbitration (higher = chosen first).
pub const USE_PRIORITY_UNLOCK: u8 = 30;
pub const USE_PRIORITY_SIGN: u8 = 20;
pub const USE_PRIORITY_ORB: u8 = 10;

/// A candidate for the single explicit-use target resolved per `I` press.
#[derive(Clone, Copy, Debug)]
pub struct UseCandidate {
    pub target: LevelEntityId,
    /// The family of the target, so `resolve_use` can route to the right query.
    pub kind: InteractionKind,
    pub priority: u8,
    pub distance: f32,
    pub facing: f32,
}

/// Deterministic arbitration: highest priority, then best facing, then shortest
/// distance, then smallest id. Independent of query / spawn order.
pub fn select_use_target(mut candidates: Vec<UseCandidate>) -> Option<UseCandidate> {
    candidates.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| b.facing.total_cmp(&a.facing))
            .then_with(|| a.distance.total_cmp(&b.distance))
            .then_with(|| a.target.cmp(&b.target))
    });
    candidates.first().copied()
}

/// The winner of this frame's interact press, filled by the Detect phase.
#[derive(Resource, Default)]
pub struct UseSelection {
    pub pressed: bool,
    pub selected: Option<LevelEntityId>,
    /// The interaction kind of the selected target, mirroring `selected`.
    pub kind: Option<InteractionKind>,
}

/// Axis-aligned overlap between two volumes.
pub fn aabb_overlap(a_center: Vec3, a_he: Vec3, b_center: Vec3, b_he: Vec3) -> bool {
    (a_center.x - b_center.x).abs() < a_he.x + b_he.x
        && (a_center.y - b_center.y).abs() < a_he.y + b_he.y
        && (a_center.z - b_center.z).abs() < a_he.z + b_he.z
}

/// Whether the actor (center `pos`, half-extents `he`) overlaps `volume`.
pub fn player_overlaps_volume(pos: Vec3, he: Vec3, volume: Vec3, volume_he: Vec3) -> bool {
    aabb_overlap(pos, he, volume, volume_he)
}

/// Whether a target with the given activation mode fires this frame, given
/// this frame's entry latch and explicit-use selection state. `TouchEnter` and
/// `TopContact` targets fire on the single entry frame; `UsePressed` targets
/// fire only when the player pressed interact on them; `Continuous` targets
/// (fans) apply every frame while active.
pub fn activation_fires(mode: ActivationMode, entered: bool, use_selected: bool) -> bool {
    match mode {
        ActivationMode::TouchEnter | ActivationMode::TopContact => entered,
        ActivationMode::UsePressed => use_selected,
        ActivationMode::Continuous => true,
    }
}

/// Whether any runtime solid other than `exclude` overlaps the volume. Gates
/// use this to refuse closing into another solid object (and never into their
/// own doorway).
pub fn solid_blocks(
    solids: &RuntimeSolids,
    exclude: LevelEntityId,
    center: Vec3,
    he: Vec3,
) -> bool {
    solids.solids.iter().any(|s| {
        s.owner != exclude && {
            let (c, sh) = rotated_box_aabb(s.center, s.shape.half_extents(), s.rotation);
            aabb_overlap(c, sh, center, he)
        }
    })
}

/// Top-contact test used by floor-like interactions (launch pads, crumble
/// plates, stomps). Fires only when the actor's feet were at/above the object
/// top last frame and are at/below the tolerance band this frame while moving
/// downward (or at rest), with horizontal footprint overlap. This prevents
/// activation through ceilings/floors, from the side, or while rising.
pub fn crossed_platform_top(
    pos: Vec3,
    prev_pos: Vec3,
    he: Vec3,
    platform_center: Vec3,
    platform_he: Vec3,
    top: f32,
    tolerance: f32,
    vel_y: f32,
) -> bool {
    if (pos.x - platform_center.x).abs() >= he.x + platform_he.x {
        return false;
    }
    if (pos.z - platform_center.z).abs() >= he.z + platform_he.z {
        return false;
    }
    let feet = pos.y - he.y;
    let prev_feet = prev_pos.y - he.y;
    vel_y <= 0.05
        && prev_feet + 1e-4 >= top - tolerance
        && feet <= top + tolerance
        && prev_feet >= feet - 1e-4
}

/// Whether an AABB body overlaps the doorway volume a gate closes through.
pub fn body_blocks_gate(pos: Vec3, he: Vec3, gate_center: Vec3, gate_he: Vec3) -> bool {
    aabb_overlap(pos, he, gate_center, gate_he)
}

/// Whether any body (player / crate / prowler) is inside the doorway volume.
pub fn gateway_blocked(
    bodies: impl IntoIterator<Item = (Vec3, Vec3)>,
    gate_center: Vec3,
    gate_he: Vec3,
) -> bool {
    bodies
        .into_iter()
        .any(|(pos, he)| body_blocks_gate(pos, he, gate_center, gate_he))
}

pub fn player_can_use_target(
    player_pos: Vec3,
    player_forward: Vec3,
    target_pos: Vec3,
    max_dist: f32,
    facing_dot: f32,
) -> bool {
    let flat = (target_pos - player_pos) * Vec3::new(1.0, 0.0, 1.0);
    let d = flat.length();
    if d > max_dist || d < 1e-4 {
        return false;
    }
    player_forward.dot(flat / d) >= facing_dot
}

/// Armor cap for the player.
pub const MAX_ARMOR: u8 = 3;

/// A heal pickup is consumed only when armor is below the cap.
pub fn heal_allowed(armor: u8) -> bool {
    armor < MAX_ARMOR
}

/// Damage resolution: armor is consumed first; `false` means the player must
/// respawn (the hit punched through the armor).
pub fn armor_hit(armor: u8, amount: u8) -> (u8, bool) {
    let new_armor = armor.saturating_sub(amount);
    (new_armor, armor >= amount)
}

/// Cap an accumulated fan force so stacking multiple fans cannot explode the
/// player.
pub fn cap_fan_force(force: Vec3, cap: f32) -> Vec3 {
    let len = force.length();
    if len > cap { force / len * cap } else { force }
}

pub const MAX_FAN_FORCE: f32 = 30.0;

const SWITCH_VOLUME_HE: Vec3 = Vec3::new(0.7, 0.6, 0.7);
const BUMPER_VOLUME_HE: Vec3 = Vec3::new(0.8, 0.8, 0.8);
const TELEPORT_VOLUME_HE: Vec3 = Vec3::new(1.1, 1.1, 1.1);
const ORB_VOLUME_HE: Vec3 = Vec3::new(1.0, 1.0, 1.0);
const CANNON_VOLUME_HE: Vec3 = Vec3::new(0.8, 1.2, 0.8);
const LOCK_GATE_VOLUME_HE: Vec3 = Vec3::new(1.3, 1.3, 1.3);
const PAD_TOP_OFFSET: f32 = 0.15;
const PAD_HE: Vec3 = Vec3::new(0.45, 0.1, 0.45);
const PLATE_TOP_OFFSET: f32 = 0.12;
const PLATE_HE: Vec3 = Vec3::new(0.5, 0.12, 0.5);
const TOP_TOLERANCE: f32 = 0.2;

const THROWN_MIN_SPEED: f32 = 4.0;

/// Advance the contact latch and clear the arbitrated use selection. Must be
/// the first system of the Detect phase.
pub fn begin_interaction_frame(world: &mut World, entities_rebuilt: bool) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    if entities_rebuilt {
        world.resource_mut::<InteractionMemory>().reset();
    }
    world.resource_mut::<InteractionMemory>().begin_frame();
    world.resource_mut::<ForcedMotionRequests>().clear();
}

/// Collect the single explicit-use target for this frame's `I` press: lock
/// gates, then signs, then trigger orbs, scored by priority / facing /
/// distance, fully independent of query order.
pub fn gather_use_targets(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    {
        let mut use_sel = world.resource_mut::<UseSelection>();
        use_sel.pressed = false;
        use_sel.selected = None;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play
        || world.resource::<MakerUi>().sign_dialog_open
    {
        return;
    }
    if !world.resource::<PlayIntent>().interact_pressed {
        return;
    }
    let player_forward = {
        let (sin, cos) = world.resource::<CameraRig>().yaw.sin_cos();
        Vec3::new(-sin, 0.0, -cos)
    };
    let pt = {
        let mut q = world.query_filtered::<&PlayerTransform, With<Player>>();
        let Ok(pt) = q.single(world) else {
            return;
        };
        pt.translation
    };

    let facing = |target: Vec3| -> Option<(f32, f32)> {
        if !player_can_use_target(pt, player_forward, target, 1.8, 0.2) {
            return None;
        }
        let flat = (target - pt) * Vec3::new(1.0, 0.0, 1.0);
        let d = flat.length();
        Some((d, player_forward.dot(flat / d)))
    };

    let mut candidates = Vec::new();

    {
        let mut q = world.query::<(&LevelEnt, &Transform, &LockGate)>();
        for (ent, tf, gate) in q.iter(world) {
            if gate.open {
                continue;
            }
            if let Some((distance, f)) = facing(tf.translation) {
                candidates.push(UseCandidate {
                    target: ent.id,
                    kind: InteractionKind::Unlock,
                    priority: USE_PRIORITY_UNLOCK,
                    distance,
                    facing: f,
                });
            }
        }
    }

    {
        let mut q = world.query::<(&LevelEnt, &Transform, &Sign)>();
        for (ent, tf, sign) in q.iter(world) {
            let Some((distance, f)) = facing(tf.translation) else {
                continue;
            };
            let sign_forward = Vec3::new(-sign.yaw_rad.sin(), 0.0, -sign.yaw_rad.cos());
            let to_player = pt - tf.translation;
            let to_player_n = (to_player * Vec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
            if sign_forward.dot(to_player_n) < 0.2 {
                continue;
            }
            candidates.push(UseCandidate {
                target: ent.id,
                kind: InteractionKind::Sign,
                priority: USE_PRIORITY_SIGN,
                distance,
                facing: f,
            });
        }
    }

    {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<TriggerOrb>>();
        for (ent, tf) in q.iter(world) {
            if let Some((distance, f)) = facing(tf.translation) {
                candidates.push(UseCandidate {
                    target: ent.id,
                    kind: InteractionKind::Trigger,
                    priority: USE_PRIORITY_ORB,
                    distance,
                    facing: f,
                });
            }
        }
    }

    let winner = select_use_target(candidates);
    let mut use_sel = world.resource_mut::<UseSelection>();
    use_sel.pressed = true;
    use_sel.selected = winner.map(|c| c.target);
    use_sel.kind = winner.map(|c| c.kind);
}

pub fn contact_he(player: &Player) -> Vec3 {
    if player.crouched {
        Vec3::new(
            player.half_extents.x,
            player.half_extents.y * 0.55,
            player.half_extents.z,
        )
    } else {
        player.half_extents
    }
}

/// Compute this frame's player (and throwable) contacts against every
/// touch-based interaction target.
pub fn detect_contacts(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let (pos, prev_pos, he, step_vy) = {
        let mut q = world.query_filtered::<(&PlayerTransform, &Player), With<Player>>();
        let Ok((pt, player)) = q.single(world) else {
            return;
        };
        let pos = pt.translation;
        // The pose from the start of this frame's collide step: reconstructing it
        // from `pos - velocity * dt` is wrong after a landing, because collision
        // already zeroed velocity.y.
        let prev_pos = if player.pre_move_pos == Vec3::ZERO {
            pos
        } else {
            player.pre_move_pos
        };
        let he = contact_he(player);
        // Vertical motion this collide step (survives y-velocity zeroing on
        // landing, which the controller does before Detect runs).
        let step_vy = pos.y - prev_pos.y;
        (pos, prev_pos, he, step_vy)
    };

    let mut keys: Vec<InteractionKey> = Vec::new();

    {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<OnOffSwitch>>();
        for (ent, tf) in q.iter(world) {
            if player_overlaps_volume(pos, he, tf.translation + Vec3::Y * 0.1, SWITCH_VOLUME_HE) {
                keys.push(InteractionKey::player(ent.id));
            }
        }
    }
    {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<Bumper>>();
        for (ent, tf) in q.iter(world) {
            if player_overlaps_volume(pos, he, tf.translation, BUMPER_VOLUME_HE) {
                keys.push(InteractionKey::player(ent.id));
            }
        }
    }
    {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<Teleporter>>();
        for (ent, tf) in q.iter(world) {
            if player_overlaps_volume(pos, he, tf.translation, TELEPORT_VOLUME_HE) {
                keys.push(InteractionKey::player(ent.id));
            }
        }
    }
    {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<TriggerOrb>>();
        for (ent, tf) in q.iter(world) {
            if player_overlaps_volume(pos, he, tf.translation, ORB_VOLUME_HE) {
                keys.push(InteractionKey::player(ent.id));
            }
        }
    }
    {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<Cannon>>();
        for (ent, tf) in q.iter(world) {
            if player_overlaps_volume(pos, he, tf.translation, CANNON_VOLUME_HE) {
                keys.push(InteractionKey::player(ent.id));
            }
        }
    }
    {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<LockGate>>();
        for (ent, tf) in q.iter(world) {
            if player_overlaps_volume(pos, he, tf.translation, LOCK_GATE_VOLUME_HE) {
                keys.push(InteractionKey::player(ent.id));
            }
        }
    }
    {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<LaunchPad>>();
        for (ent, tf) in q.iter(world) {
            let top = tf.translation.y + PAD_TOP_OFFSET;
            if crossed_platform_top(
                pos,
                prev_pos,
                he,
                tf.translation,
                PAD_HE,
                top,
                TOP_TOLERANCE,
                step_vy,
            ) {
                keys.push(InteractionKey::player(ent.id));
            }
        }
    }
    {
        let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<CrumblePlate>>();
        for (ent, tf) in q.iter(world) {
            let top = tf.translation.y + PLATE_TOP_OFFSET;
            if crossed_platform_top(
                pos,
                prev_pos,
                he,
                tf.translation,
                PLATE_HE,
                top,
                TOP_TOLERANCE,
                step_vy,
            ) {
                keys.push(InteractionKey::player(ent.id));
            }
        }
    }

    let thrown: Vec<(Entity, Vec3, f32)> = {
        let mut q = world
            .query_filtered::<(Entity, &Transform, &Velocity), (With<Throwable>, Without<Player>)>(
            );
        q.iter(world)
            .map(|(e, tf, vel)| (e, tf.translation, vel.linear.length()))
            .collect()
    };
    for (crate_e, ctf, speed) in thrown {
        if speed < THROWN_MIN_SPEED {
            continue;
        }
        let c_he = Vec3::splat(0.4);
        {
            let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<OnOffSwitch>>();
            for (ent, s_tf) in q.iter(world) {
                if player_overlaps_volume(
                    ctf,
                    c_he,
                    s_tf.translation + Vec3::Y * 0.1,
                    SWITCH_VOLUME_HE,
                ) {
                    keys.push(InteractionKey {
                        actor: crate_e.index_u32(),
                        target: ent.id,
                    });
                }
            }
        }
        {
            let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<TriggerOrb>>();
            for (ent, o_tf) in q.iter(world) {
                if player_overlaps_volume(ctf, c_he, o_tf.translation, ORB_VOLUME_HE) {
                    keys.push(InteractionKey {
                        actor: crate_e.index_u32(),
                        target: ent.id,
                    });
                }
            }
        }
        {
            let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<LaunchPad>>();
            for (ent, p_tf) in q.iter(world) {
                if player_overlaps_volume(ctf, c_he, p_tf.translation, PAD_HE) {
                    keys.push(InteractionKey {
                        actor: crate_e.index_u32(),
                        target: ent.id,
                    });
                }
            }
        }
        {
            let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<Bumper>>();
            for (ent, b_tf) in q.iter(world) {
                if player_overlaps_volume(ctf, c_he, b_tf.translation, BUMPER_VOLUME_HE) {
                    keys.push(InteractionKey {
                        actor: crate_e.index_u32(),
                        target: ent.id,
                    });
                }
            }
        }
        {
            let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<CrumblePlate>>();
            for (ent, p_tf) in q.iter(world) {
                // Simple AABB for thrown crates is fine.
                if player_overlaps_volume(ctf, c_he, p_tf.translation, PLATE_HE) {
                    keys.push(InteractionKey {
                        actor: crate_e.index_u32(),
                        target: ent.id,
                    });
                }
            }
        }
    }

    let mut memory = world.resource_mut::<InteractionMemory>();
    for key in keys {
        memory.touch(key);
    }
}

/// Generate damage/break requests from player and thrown-crate contact.
pub fn detect_damage(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let (player_e, pt, he, player_bottom, falling, slamming) = {
        let mut q = world.query_filtered::<(Entity, &PlayerTransform, &Player), With<Player>>();
        let Ok((player_e, pt, player)) = q.single(world) else {
            return;
        };
        let he = contact_he(player);
        (
            player_e,
            pt.translation,
            he,
            pt.translation.y - he.y,
            player.velocity.y.min(player.fall_speed),
            player.slamming,
        )
    };

    let mut requests: Vec<DamageRequest> = Vec::new();

    {
        let mut q = world.query_filtered::<(Entity, &Transform), With<Prowler>>();
        for (prow_e, prow_tf) in q.iter(world) {
            let d = (pt - prow_tf.translation).abs();
            let ph = Vec3::splat(0.35);
            let overlap = d.x < he.x + ph.x && d.y < he.y + ph.y && d.z < he.z + ph.z;
            if !overlap {
                continue;
            }
            let is_stomp = falling < -0.5 && player_bottom > prow_tf.translation.y - 0.05;
            if is_stomp {
                requests.push(DamageRequest {
                    target: prow_e,
                    source: player_e,
                    amount: 1,
                    attack: Some(AttackKind::Stomp),
                });
            } else {
                requests.push(DamageRequest {
                    target: player_e,
                    source: prow_e,
                    amount: 1,
                    attack: None,
                });
            }
        }
    }

    {
        let mut q = world.query_filtered::<(Entity, &Transform, &CrateProp), Without<Player>>();
        for (c_e, c_tf, prop) in q.iter(world) {
            if !prop.breakable {
                continue;
            }
            let d = (pt - c_tf.translation).abs();
            let ph = Vec3::splat(0.45);
            let overlap = d.x < he.x + ph.x && d.y < he.y + ph.y && d.z < he.z + ph.z;
            if !overlap {
                continue;
            }
            let stomp = falling < -0.5 && player_bottom > c_tf.translation.y - 0.05;
            if stomp {
                requests.push(DamageRequest {
                    target: c_e,
                    source: player_e,
                    amount: 1,
                    attack: Some(AttackKind::Stomp),
                });
            } else if slamming {
                requests.push(DamageRequest {
                    target: c_e,
                    source: player_e,
                    amount: 1,
                    attack: Some(AttackKind::Slam),
                });
            }
        }
    }

    let thrown: Vec<(Entity, Vec3, f32)> = {
        let mut q = world
            .query_filtered::<(Entity, &Transform, &Velocity), (With<Throwable>, Without<Player>)>(
            );
        q.iter(world)
            .map(|(e, tf, vel)| (e, tf.translation, vel.linear.length()))
            .collect()
    };
    for (crate_e, ctf, speed) in thrown {
        if speed < THROWN_MIN_SPEED {
            continue;
        }
        let c_he = Vec3::splat(0.4);
        {
            let mut q = world.query_filtered::<(Entity, &Transform), With<Prowler>>();
            for (prow_e, prow_tf) in q.iter(world) {
                let d = (ctf - prow_tf.translation).abs();
                if d.x < c_he.x + 0.45 && d.y < c_he.y + 0.45 && d.z < c_he.z + 0.45 {
                    requests.push(DamageRequest {
                        target: prow_e,
                        source: crate_e,
                        amount: 1,
                        attack: Some(AttackKind::ThrownImpact),
                    });
                }
            }
        }
        {
            let mut q = world.query_filtered::<(Entity, &Transform, &CrateProp), Without<Player>>();
            for (c_e, c_tf, prop) in q.iter(world) {
                if !prop.breakable {
                    continue;
                }
                let d = (ctf - c_tf.translation).abs();
                if d.x < c_he.x + 0.45 && d.y < c_he.y + 0.45 && d.z < c_he.z + 0.45 {
                    requests.push(DamageRequest {
                        target: c_e,
                        source: crate_e,
                        amount: 1,
                        attack: Some(AttackKind::ThrownImpact),
                    });
                }
            }
        }
    }

    let mut store = world.resource_mut::<DamageRequests>();
    for request in requests {
        store.push(request);
    }
}

/// Apply the single winning forced-motion request (respawn, teleport, cannon,
/// launch pad, bumper) with consistent bookkeeping.
pub fn resolve_forced_motion(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let motion = {
        let mut requests = world.resource_mut::<ForcedMotionRequests>();
        let Some(motion) = requests.winner().cloned() else {
            requests.clear();
            return;
        };
        let position_applied = motion.position.is_some() || motion.respawn;
        requests.requests.clear();
        requests.position_applied = position_applied;
        motion
    };
    let player_e = {
        let mut q = world.query_filtered::<Entity, With<Player>>();
        let Ok(e) = q.single(world) else {
            return;
        };
        e
    };

    if motion.respawn {
        // A held crate is left where the player died: drop it back to a normal
        // dynamic crate so it doesn't float mid-air or block the respawn.
        let held: Vec<Entity> = {
            let mut q = world.query_filtered::<Entity, With<Held>>();
            q.iter(world).collect()
        };
        for e in held {
            if world.get::<Throwable>(e).is_some() {
                super::rapier::make_dynamic(world, e, None);
            } else {
                world.entity_mut(e).remove::<Held>();
                world.entity_mut(e).insert(Velocity::zero());
            }
        }
        let status = if motion.source.target == NO_TARGET {
            "You fell off the level!"
        } else {
            "Back to checkpoint!"
        };
        world.resource_mut::<MakerUi>().set_status(status);
        respawn(world, player_e);
        return;
    }
    let mut q =
        world.query_filtered::<(&mut PlayerTransform, &mut Player, &mut MoveState), With<Player>>();
    let Ok((mut tf, mut player, mut move_state)) = q.get_mut(world, player_e) else {
        return;
    };

    if let Some(pos) = motion.position {
        tf.translation = pos;
    }
    if let Some(vel) = motion.velocity {
        player.velocity = vel;
    }

    player.slamming = false;
    player.gripping = false;
    player.grip_top = 0.0;
    player.move_mode = PlayerMoveMode::Normal;
    player.coyote = 0.0;
    player.on_ground = false;
    player.was_on_ground = false;
    player.ground_plate = None;
    player.plate_vel = Vec3::ZERO;
    player.fall_speed = 0.0;
    player.hang_cooldown = 0.25;
    player.pre_move_pos = tf.translation;
    if motion.control_lock > 0.0 {
        player.launch = player.launch.max(motion.control_lock);
    }
    move_state.action = ActionState::Launch;
    move_state.floor_normal = Vec3::Y;
}

fn landing_velocity(impulse: f32, yaw_rad: f32) -> Vec3 {
    let dir = Quat::from_rotation_y(yaw_rad) * Vec3::NEG_Z;
    dir * impulse + Vec3::Y * (impulse * 0.35)
}

/// Launch pads fire exactly once per landing (top contact + entry latch).
pub fn resolve_launch_pads(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let pads: Vec<(LevelEntityId, f32, f32)> = {
        let mut q = world.query::<(&LevelEnt, &LaunchPad)>();
        q.iter(world)
            .map(|(ent, pad)| (ent.id, pad.impulse, pad.yaw_rad))
            .collect()
    };
    let crates: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, (With<Throwable>, Without<Player>)>();
        q.iter(world).collect()
    };
    let mut pushes: Vec<ForcedMotion> = Vec::new();
    let mut crate_vels: Vec<(Entity, Vec3)> = Vec::new();
    {
        let memory = world.resource::<InteractionMemory>();
        for (id, impulse, yaw) in pads.iter().copied() {
            let key = InteractionKey::player(id);
            if activation_fires(ActivationMode::TopContact, memory.entered(key), false) {
                pushes.push(ForcedMotion {
                    source: key,
                    priority: PRIORITY_LAUNCH,
                    position: None,
                    velocity: Some(landing_velocity(impulse, yaw)),
                    control_lock: 0.9,
                    respawn: false,
                });
            }
        }
        for crate_e in crates.iter().copied() {
            for (id, impulse, yaw) in pads.iter().copied() {
                let key = InteractionKey {
                    actor: crate_e.index_u32(),
                    target: id,
                };
                if memory.entered(key) {
                    crate_vels.push((crate_e, landing_velocity(impulse, yaw)));
                }
            }
        }
    }
    for motion in pushes {
        world.resource_mut::<ForcedMotionRequests>().push(motion);
    }
    if !crate_vels.is_empty() {
        let mut q = world.query_filtered::<&mut Velocity, (With<Throwable>, Without<Player>)>();
        for (e, vel) in crate_vels.iter().copied() {
            if let Ok(mut v) = q.get_mut(world, e) {
                v.linear = vel;
            }
        }
        // The body writeback overwrites Velocity each frame, so the impulse
        // has to reach Rapier as a request too.
        for (e, vel) in crate_vels {
            world.entity_mut(e).insert(VelocityRequest3d {
                linear: vel,
                angular: Vec3::ZERO,
            });
        }
    }
}

/// Bumpers use entry contact and separate horizontal direction from upward
/// bias.
pub fn resolve_bumpers(world: &mut World) {
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
    let bumpers: Vec<(LevelEntityId, Vec3, f32)> = {
        let mut q = world.query::<(&LevelEnt, &Transform, &Bumper)>();
        q.iter(world)
            .map(|(ent, tf, bumper)| (ent.id, tf.translation, bumper.strength))
            .collect()
    };
    let crates: Vec<(Entity, Vec3)> = {
        let mut q =
            world.query_filtered::<(Entity, &Transform), (With<Throwable>, Without<Player>)>();
        q.iter(world).map(|(e, tf)| (e, tf.translation)).collect()
    };
    let mut pushes: Vec<ForcedMotion> = Vec::new();
    let mut crate_vels: Vec<(Entity, Vec3)> = Vec::new();
    let mut boing = false;
    {
        let memory = world.resource::<InteractionMemory>();
        for (id, bumper_pos, strength) in bumpers.iter().copied() {
            let key = InteractionKey::player(id);
            if !activation_fires(ActivationMode::TouchEnter, memory.entered(key), false) {
                continue;
            }
            let delta = pt - bumper_pos;
            let horiz = Vec3::new(delta.x, 0.0, delta.z).normalize_or_zero();
            let vel = if horiz == Vec3::ZERO {
                Vec3::Y * strength
            } else {
                horiz * strength + Vec3::Y * (strength * 0.55)
            };
            pushes.push(ForcedMotion {
                source: key,
                priority: PRIORITY_BUMPER,
                position: None,
                velocity: Some(vel),
                control_lock: 0.3,
                respawn: false,
            });
            boing = true;
        }
        for (crate_e, ctf) in crates.iter().copied() {
            for (id, bumper_pos, strength) in bumpers.iter().copied() {
                let key = InteractionKey {
                    actor: crate_e.index_u32(),
                    target: id,
                };
                if !memory.entered(key) {
                    continue;
                }
                let delta = ctf - bumper_pos;
                let horiz = Vec3::new(delta.x, 0.0, delta.z).normalize_or_zero();
                crate_vels.push((
                    crate_e,
                    if horiz == Vec3::ZERO {
                        Vec3::Y * strength
                    } else {
                        horiz * strength + Vec3::Y * (strength * 0.55)
                    },
                ));
            }
        }
    }
    for motion in pushes {
        world.resource_mut::<ForcedMotionRequests>().push(motion);
    }
    if !crate_vels.is_empty() {
        let mut q = world.query_filtered::<&mut Velocity, (With<Throwable>, Without<Player>)>();
        for (e, vel) in crate_vels.iter().copied() {
            if let Ok(mut v) = q.get_mut(world, e) {
                v.linear = vel;
            }
        }
        for (e, vel) in crate_vels {
            world.entity_mut(e).insert(VelocityRequest3d {
                linear: vel,
                angular: Vec3::ZERO,
            });
        }
    }
    if boing {
        world.resource_mut::<MakerUi>().set_status("Boing!");
    }
}

/// Cannons fire once on entry and enter a launch state consistent with launch
/// pads.
pub fn resolve_cannons(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let cannons: Vec<(LevelEntityId, Vec3, Vec3, f32)> = {
        let mut q = world.query::<(&LevelEnt, &Transform, &Cannon)>();
        q.iter(world)
            .map(|(ent, tf, cannon)| (ent.id, tf.translation, cannon.target, cannon.arc))
            .collect()
    };
    let mut pushes: Vec<ForcedMotion> = Vec::new();
    let mut status: Option<&'static str> = None;
    {
        let memory = world.resource::<InteractionMemory>();
        for (id, tf_pos, target, arc) in cannons.iter().copied() {
            let key = InteractionKey::player(id);
            if !activation_fires(ActivationMode::TouchEnter, memory.entered(key), false) {
                continue;
            }
            let delta = target - tf_pos;
            let horiz = (delta * Vec3::new(1.0, 0.0, 1.0)).length();
            if horiz < 0.01 {
                status = Some("Cannon needs a target cell.");
                continue;
            }
            let g = 25.0;
            let t_peak = (2.0 * arc / g).sqrt();
            let t = 2.0 * t_peak;
            let dir = (delta * Vec3::new(1.0, 0.0, 1.0)).normalize();
            let mut v = dir * (horiz / t);
            v.y = g * t_peak + delta.y / t;

            pushes.push(ForcedMotion {
                source: key,
                priority: PRIORITY_CANNON,
                position: None,
                velocity: Some(v),
                control_lock: 0.5,
                respawn: false,
            });
            status = Some("Fired!");
        }
    }
    for motion in pushes {
        world.resource_mut::<ForcedMotionRequests>().push(motion);
    }
    if let Some(status) = status {
        world.resource_mut::<MakerUi>().set_status(status);
    }
}

/// The destination exit lock is held while the player stays inside the
/// destination volume; it clears once they leave.
fn update_teleport_exit_lock(memory: &mut InteractionMemory, inside_dest: bool) {
    if let Some(_) = memory.teleport_exit_lock {
        if !inside_dest {
            memory.teleport_exit_lock = None;
        }
    }
}

/// Deterministic teleport destination: the next endpoint by sorted
/// `LevelEntityId`, wrapping. Independent of query / spawn order.
fn teleport_destination<'a>(
    endpoints: &'a [(LevelEntityId, Vec3)],
    from_id: LevelEntityId,
) -> Option<&'a (LevelEntityId, Vec3)> {
    if endpoints.len() < 2 {
        return None;
    }
    let mut sorted: Vec<&(LevelEntityId, Vec3)> = endpoints.iter().collect();
    sorted.sort_by_key(|(id, _)| *id);
    let idx = sorted
        .iter()
        .position(|(id, _)| *id == from_id)
        .unwrap_or(0);
    Some(sorted[(idx + 1) % sorted.len()])
}

/// Teleporters: two-endpoint links (deterministic for legacy 3+), destination
/// clearance validation, and an exit lock that prevents bouncing back until
/// the player leaves the destination volume.
#[allow(clippy::too_many_arguments)]
pub fn resolve_teleporters(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let (pt, he) = {
        let mut q = world.query_filtered::<(&PlayerTransform, &Player), With<Player>>();
        let Ok((pt, player)) = q.single(world) else {
            return;
        };
        (pt.translation, contact_he(player))
    };

    // Clear the exit lock once the player has left the destination volume.
    let lock = world.resource::<InteractionMemory>().teleport_exit_lock;
    if let Some(lock) = lock {
        let inside_dest = {
            let mut q = world.query_filtered::<(&LevelEnt, &Transform), With<Teleporter>>();
            q.iter(world).any(|(ent, tf)| {
                ent.id == lock.target
                    && player_overlaps_volume(pt, he, tf.translation, TELEPORT_VOLUME_HE)
            })
        };
        let mut memory = world.resource_mut::<InteractionMemory>();
        update_teleport_exit_lock(&mut memory, inside_dest);
    }
    if world
        .resource::<InteractionMemory>()
        .teleport_exit_lock
        .is_some()
    {
        return;
    }

    let mut grouped: std::collections::HashMap<u32, Vec<(LevelEntityId, Vec3)>> =
        Default::default();
    let mut teleporter_q =
        world.query_filtered::<(&LevelEnt, &Transform, &Teleporter), Without<Player>>();
    for (ent, tf, tp) in teleporter_q.iter(world) {
        if tp.link != 0 {
            grouped
                .entry(tp.link)
                .or_default()
                .push((ent.id, tf.translation));
        }
    }

    let mut warp: Option<(LevelEntityId, Vec3, InteractionKey)> = None;
    let mut status: Option<&'static str> = None;
    {
        let memory = world.resource::<InteractionMemory>();
        let level = world.resource::<LevelDocument>();
        let solids = world.resource::<RuntimeSolids>();
        for (ent, _tf, tp) in teleporter_q.iter(world) {
            if tp.link == 0 {
                continue;
            }
            let key = InteractionKey::player(ent.id);
            if !activation_fires(ActivationMode::TouchEnter, memory.entered(key), false) {
                continue;
            }
            let endpoints = grouped.get(&tp.link).map(Vec::as_slice).unwrap_or(&[]);
            let Some(dest) = teleport_destination(endpoints, ent.id) else {
                status = Some("Teleporter needs a linked pair.");
                continue;
            };
            let dest_pos = dest.1 + Vec3::Y * 0.9;

            if !teleport_clear(dest_pos, he, level, solids) {
                status = Some("Teleporter destination blocked.");
                continue;
            }

            warp = Some((dest.0, dest_pos, key));
            status = Some("Warped!");
            break;
        }
    }

    if let Some((dest_id, dest_pos, key)) = warp {
        world.resource_mut::<InteractionMemory>().teleport_exit_lock =
            Some(InteractionKey::player(dest_id));
        world
            .resource_mut::<ForcedMotionRequests>()
            .push(ForcedMotion {
                source: key,
                priority: PRIORITY_TELEPORT,
                position: Some(dest_pos),
                velocity: Some(Vec3::ZERO),
                control_lock: 0.15,
                respawn: false,
            });
    }
    if let Some(status) = status {
        world.resource_mut::<MakerUi>().set_status(status);
    }
}

fn teleport_clear(pos: Vec3, he: Vec3, level: &LevelDocument, solids: &RuntimeSolids) -> bool {
    if aabb_hits_solid(level, pos, he) {
        return false;
    }
    for solid in &solids.solids {
        let (center, s_he) =
            rotated_box_aabb(solid.center, solid.shape.half_extents(), solid.rotation);
        if aabb_overlap(pos, he, center, s_he) {
            return false;
        }
    }
    true
}

/// Pulse a trigger orb's channel (shared by explicit use and physical touch).
fn fire_orb(world: &mut World, e: Entity) {
    let Some(orb) = world.get::<TriggerOrb>(e) else {
        return;
    };
    let (channel, cooldown) = (orb.channel, orb.cooldown);
    if channel == 0 {
        world
            .resource_mut::<MakerUi>()
            .set_status("Orb has no channel set.");
        return;
    }
    if let Some(mut orb) = world.get_mut::<TriggerOrb>(e) {
        orb.timer = cooldown;
    }
    {
        let mut link = world.resource_mut::<LinkState>();
        let clock = link.clock;
        link.pulses.insert(channel, clock);
    }
    world.resource_mut::<Trauma>().add(0.1);
    world
        .resource_mut::<MakerUi>()
        .set_status(format!("Channel {} triggered!", channel));
}

/// Signs, trigger orbs and lock gates resolve from the single arbitrated use
/// target; orbs may also fire on physical entry (player or thrown crate).
/// Lock gates consume a key only when explicitly selected, never every frame.
#[allow(clippy::too_many_arguments)]
pub fn resolve_use(world: &mut World, dt: f32) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    world.resource_mut::<LinkState>().clock += dt;
    let player_e = {
        let mut q = world.query_filtered::<Entity, With<Player>>();
        let Ok(e) = q.single(world) else {
            return;
        };
        e
    };

    if world.resource::<MakerUi>().sign_dialog_open {
        let dismiss = {
            let intent = world.resource::<PlayIntent>();
            intent.interact_pressed || intent.jump_pressed
        };
        if dismiss {
            let mut ui = world.resource_mut::<MakerUi>();
            ui.sign_dialog_open = false;
            ui.sign_dialog_lines.clear();
        }
        return;
    }

    let (pressed, selected, kind) = {
        let use_sel = world.resource::<UseSelection>();
        (use_sel.pressed, use_sel.selected, use_sel.kind)
    };

    if !pressed || selected.is_none() {
        // No explicit press: physical contact may still fire orbs.
        let mut orbs_q =
            world.query_filtered::<(Entity, &LevelEnt, &TriggerOrb), Without<Player>>();
        let mut timers: Vec<(Entity, f32)> = Vec::new();
        let mut fires: Vec<Entity> = Vec::new();
        {
            let memory = world.resource::<InteractionMemory>();
            for (e, ent, orb) in orbs_q.iter(world) {
                let timer = (orb.timer - dt).max(0.0);
                timers.push((e, timer));
                if timer > 0.0 {
                    continue;
                }
                if memory.any_entered_target(ent.id) {
                    fires.push(e);
                }
            }
        }
        for (e, timer) in timers {
            if let Some(mut orb) = world.get_mut::<TriggerOrb>(e) {
                orb.timer = timer;
            }
        }
        for e in fires {
            fire_orb(world, e);
        }
        return;
    }

    let selected = selected.unwrap();

    // Route by the kind of the arbitrated target so only the matching family is
    // scanned; the priority order (gate > sign > orb) lives in the arbitration.
    match kind {
        Some(InteractionKind::Unlock) => {
            let gate = {
                let mut q =
                    world.query_filtered::<(Entity, &LevelEnt, &LockGate), Without<Player>>();
                q.iter(world)
                    .find(|(_, ent, gate)| ent.id == selected && !gate.open)
                    .map(|(e, _, gate)| (e, gate.link))
            };
            let Some((gate_e, link)) = gate else {
                return;
            };
            let ch = link as usize;
            let mut need_key = false;
            {
                let mut q = world.query_filtered::<&mut Player, With<Player>>();
                let Ok(mut player) = q.get_mut(world, player_e) else {
                    return;
                };
                if ch >= player.keys.len() || player.keys[ch] == 0 {
                    need_key = true;
                } else {
                    player.keys[ch] -= 1;
                }
            }
            if need_key {
                world
                    .resource_mut::<MakerUi>()
                    .set_status(format!("Need key (ch {})", link));
                return;
            }
            if let Some(mut gate) = world.get_mut::<LockGate>(gate_e) {
                gate.open = true;
                gate.open_timer = gate.open_for;
            }
            world.resource_mut::<MakerUi>().set_status("Gate unlocked!");
            return;
        }
        Some(InteractionKind::Sign) => {
            let text = {
                let mut q = world.query_filtered::<(&LevelEnt, &Sign), Without<Player>>();
                q.iter(world)
                    .find(|(ent, _)| ent.id == selected)
                    .map(|(_, sign)| sign.text.clone())
            };
            let Some(text) = text else {
                return;
            };
            let mut ui = world.resource_mut::<MakerUi>();
            ui.status.clear();
            ui.sign_dialog_open = true;
            ui.sign_dialog_lines = wrap_sign_text(&text);
            return;
        }
        Some(InteractionKind::Trigger) => {
            let orb_e = {
                let mut q = world.query_filtered::<(Entity, &LevelEnt), With<TriggerOrb>>();
                q.iter(world)
                    .find(|(_, ent)| ent.id == selected)
                    .map(|(e, _)| e)
            };
            let Some(orb_e) = orb_e else {
                return;
            };
            let timer = {
                let Some(orb) = world.get::<TriggerOrb>(orb_e) else {
                    return;
                };
                (orb.timer - dt).max(0.0)
            };
            if timer > 0.0 {
                if let Some(mut orb) = world.get_mut::<TriggerOrb>(orb_e) {
                    orb.timer = timer;
                }
                return;
            }
            fire_orb(world, orb_e);
            return;
        }
        _ => {}
    }
}

fn damage_player(world: &mut World, player_e: Entity, amount: u8) {
    let (alive, armor) = {
        let mut q = world.query_filtered::<&mut Player, With<Player>>();
        let Ok(mut player) = q.get_mut(world, player_e) else {
            return;
        };
        if player.invuln > 0.0 {
            return;
        }
        let (new_armor, alive) = armor_hit(player.armor, amount);
        player.armor = new_armor;
        if alive {
            player.invuln = 0.6;
            player.velocity.y = JUMP_SPEED * 0.55;
            player.on_ground = false;
            player.coyote = 0.0;
            player.ground_plate = None;
            player.plate_vel = Vec3::ZERO;
            (true, player.armor)
        } else {
            (false, player.armor)
        }
    };
    if alive {
        world
            .resource_mut::<MakerUi>()
            .set_status(format!("Ouch! Armor left: {}", armor));
        return;
    }
    world.resource_mut::<MakerUi>().deaths += 1;
    respawn(world, player_e);
    world.resource_mut::<Trauma>().add(0.35);
    world.resource_mut::<MakerUi>().set_status("Ouch!");
}

/// Apply this frame's unique damage/break requests. Prowler contact damages
/// the player (armor first); stomps, slams and thrown impacts defeat
/// prowlers and break breakable crates.
#[allow(clippy::too_many_arguments)]
pub fn resolve_damage(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        world.resource_mut::<DamageRequests>().clear();
        return;
    }
    let uniques = {
        let mut requests = world.resource_mut::<DamageRequests>();
        let uniques = requests.unique();
        requests.clear();
        uniques
    };
    if uniques.is_empty() {
        return;
    }
    let player_e = {
        let mut q = world.query_filtered::<Entity, With<Player>>();
        let Ok(e) = q.single(world) else {
            return;
        };
        e
    };
    let mut prowler_q =
        world.query_filtered::<(Entity, &LevelEnt, &Transform, Option<&Contents>), With<Prowler>>();
    let mut crate_q = world.query_filtered::<
        (Entity, &LevelEnt, &Transform, &CrateProp, Option<&Contents>),
        Without<Player>,
    >();

    for req in uniques {
        if req.target == player_e {
            if req.attack.is_none() {
                damage_player(world, player_e, req.amount);
            }
            continue;
        }

        let prowler = prowler_q
            .iter(world)
            .find(|(e, _, _, _)| *e == req.target)
            .map(|(e, ent, tf, contents)| {
                (
                    e,
                    ent.id,
                    tf.translation,
                    contents.map(|c| Contents {
                        item: c.item,
                        link: c.link,
                    }),
                )
            });
        if let Some((e, id, origin, contents)) = prowler {
            if req.attack.is_some() {
                defeat_prowler(
                    world,
                    player_e,
                    origin,
                    id,
                    e,
                    contents,
                    req.source == player_e,
                );
                continue;
            }
        }

        let crate_hit = crate_q
            .iter(world)
            .find(|(e, _, _, _, _)| *e == req.target)
            .map(|(e, ent, tf, _prop, contents)| {
                (
                    e,
                    ent.id,
                    tf.translation,
                    contents.map(|c| Contents {
                        item: c.item,
                        link: c.link,
                    }),
                )
            });
        if let Some((e, id, origin, contents)) = crate_hit
            && req.attack.is_some()
        {
            break_crate(world, origin, id, e, contents);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn defeat_prowler(
    world: &mut World,
    player_e: Entity,
    origin: Vec3,
    id: LevelEntityId,
    e: Entity,
    contents: Option<Contents>,
    player_bounce: bool,
) {
    if let Some(contents) = &contents {
        spawn_drops(world, origin, contents);
    }
    world.despawn(e);
    world.resource_mut::<EntityEntities>().0.remove(&id);
    if player_bounce {
        let mut q = world.query_filtered::<&mut Player, With<Player>>();
        if let Ok(mut player) = q.get_mut(world, player_e) {
            player.velocity.y = JUMP_SPEED * 0.8;
            player.on_ground = false;
            player.coyote = 0.0;
            player.ground_plate = None;
            player.plate_vel = Vec3::ZERO;
        }
    }
    world.resource_mut::<Trauma>().add(0.18);
    let total = {
        let mut ui = world.resource_mut::<MakerUi>();
        ui.score += 200;
        ui.score
    };
    world
        .resource_mut::<MakerUi>()
        .set_status(format!("Prowler defeated! +{total}"));
}

#[allow(clippy::too_many_arguments)]
fn break_crate(
    world: &mut World,
    origin: Vec3,
    id: LevelEntityId,
    e: Entity,
    contents: Option<Contents>,
) {
    if let Some(contents) = &contents {
        spawn_drops(world, origin, contents);
    }
    world.despawn(e);
    world.resource_mut::<EntityEntities>().0.remove(&id);
    let mut ui = world.resource_mut::<MakerUi>();
    ui.score += 50;
    ui.set_status("Crate smashed!");
}

fn respawn(world: &mut World, player: Entity) {
    world.resource_scope(|world, level: Mut<LevelDocument>| {
        let mut q = world.query::<(&mut PlayerTransform, &mut Player, &mut MoveState)>();
        if let Ok((mut transform, mut player_c, mut move_state)) = q.get_mut(world, player) {
            respawn_player(&mut transform, &mut player_c, &mut move_state, &level);
        }
    });
}

/// Hazards damage the player through the shared damage path; falling out of
/// bounds or manual reset request a full respawn.
pub fn play_hazard_goal(world: &mut World) {
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    if world.resource::<Paused>().0 || world.resource::<MakerUi>().goal_latched {
        return;
    }
    enum Outcome {
        None,
        Respawn { manual_reset: bool },
        Damage,
    }
    let (player_e, outcome) = {
        let mut q = world.query::<(Entity, &PlayerTransform, &Player)>();
        let level = world.resource::<LevelDocument>();
        let intent = world.resource::<PlayIntent>();
        let Ok((player_e, transform, player)) = q.single(world) else {
            return;
        };
        let he = contact_he(player);
        let hit_hazard = overlaps_kind(transform.translation, he, &level, BlockKind::Hazard)
            || overlaps_kind(transform.translation, he, &level, BlockKind::Spikes);
        let fell_off = transform.translation.y < -20.0;
        let bounds = level.play_bounds();
        let out_of_bounds = transform.translation.x < bounds.0.x as f32 - 0.5
            || transform.translation.x > bounds.1.x as f32 + 0.5
            || transform.translation.z < bounds.0.z as f32 - 0.5
            || transform.translation.z > bounds.1.z as f32 + 0.5;
        let manual_reset = intent.reset_pressed;
        let outcome = if fell_off || out_of_bounds || manual_reset {
            Outcome::Respawn { manual_reset }
        } else if hit_hazard && player.invuln <= 0.0 {
            Outcome::Damage
        } else {
            Outcome::None
        };
        (player_e, outcome)
    };
    match outcome {
        Outcome::None => {}
        Outcome::Respawn { manual_reset } => {
            {
                let mut ui = world.resource_mut::<MakerUi>();
                if !manual_reset {
                    ui.deaths += 1;
                }
                ui.set_status(if manual_reset {
                    "Back to checkpoint!"
                } else {
                    "You fell off the level!"
                });
            }
            respawn(world, player_e);
        }
        Outcome::Damage => {
            {
                let mut ui = world.resource_mut::<MakerUi>();
                ui.deaths += 1;
                ui.set_status("Ouch!");
            }
            world.resource_mut::<Trauma>().add(0.35);
            respawn(world, player_e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maker::entity_data::EntityKind;
    use crate::maker::interactive_blocks::{OnOffState, touch_onoff_switches};
    use crate::maker::player::spawn_player;
    use repame_shell::Sim;

    const HE: Vec3 = Vec3::new(0.3, 0.9, 0.3);

    fn player_key(id: LevelEntityId) -> InteractionKey {
        InteractionKey::player(id)
    }

    #[test]
    fn switch_toggles_once_while_standing() {
        let mut mem = InteractionMemory::default();
        let key = player_key(1);
        // Frame 1: begin contact -> entered.
        mem.begin_frame();
        mem.touch(key);
        assert!(mem.entered(key));
        // Frame 2: still touching -> NOT entered again.
        mem.begin_frame();
        mem.touch(key);
        assert!(!mem.entered(key));
        // Frame 3: still touching -> still not entered.
        mem.begin_frame();
        mem.touch(key);
        assert!(!mem.entered(key));
    }

    #[test]
    fn leaving_and_reentering_toggles_again() {
        let mut mem = InteractionMemory::default();
        let key = player_key(1);
        mem.begin_frame();
        mem.touch(key);
        assert!(mem.entered(key));
        // Leave.
        mem.begin_frame();
        assert!(!mem.entered(key));
        // Re-enter.
        mem.begin_frame();
        mem.touch(key);
        assert!(mem.entered(key));
    }

    #[test]
    fn timed_gate_does_not_consume_key_every_frame_while_nearby() {
        let mut mem = InteractionMemory::default();
        let key = player_key(9);
        // Frame 1: nearby -> entered (would consume a key).
        mem.begin_frame();
        mem.touch(key);
        assert!(mem.entered(key));
        // Frames 2-3: still nearby -> not entered again (no second key).
        mem.begin_frame();
        mem.touch(key);
        assert!(!mem.entered(key));
        mem.begin_frame();
        mem.touch(key);
        assert!(!mem.entered(key));
    }

    #[test]
    fn one_press_cannot_activate_sign_and_orb() {
        let winner = select_use_target(vec![
            UseCandidate {
                target: 42,
                kind: InteractionKind::Trigger,
                priority: USE_PRIORITY_ORB,
                distance: 1.0,
                facing: 0.9,
            },
            UseCandidate {
                target: 10,
                kind: InteractionKind::Sign,
                priority: USE_PRIORITY_SIGN,
                distance: 1.0,
                facing: 0.9,
            },
        ]);
        assert_eq!(winner.map(|c| c.target), Some(10));
        assert_eq!(winner.map(|c| c.kind), Some(InteractionKind::Sign));
        // Exactly one winner: a press resolves to a single target.
        assert_ne!(winner.map(|c| c.target), Some(42));
    }

    #[test]
    fn use_selection_independent_of_candidate_order() {
        let c1 = vec![
            UseCandidate {
                target: 7,
                kind: InteractionKind::Sign,
                priority: USE_PRIORITY_SIGN,
                distance: 1.5,
                facing: 0.6,
            },
            UseCandidate {
                target: 3,
                kind: InteractionKind::Trigger,
                priority: USE_PRIORITY_ORB,
                distance: 1.0,
                facing: 0.95,
            },
        ];
        let c2 = vec![
            UseCandidate {
                target: 3,
                kind: InteractionKind::Trigger,
                priority: USE_PRIORITY_ORB,
                distance: 1.0,
                facing: 0.95,
            },
            UseCandidate {
                target: 7,
                kind: InteractionKind::Sign,
                priority: USE_PRIORITY_SIGN,
                distance: 1.5,
                facing: 0.6,
            },
        ];
        let r1 = select_use_target(c1);
        let r2 = select_use_target(c2);
        assert_eq!(r1.map(|c| c.target), r2.map(|c| c.target));
        // Higher priority (sign, 20) wins over the orb (10).
        assert_eq!(r1.map(|c| c.target), Some(7));
    }

    #[test]
    fn launch_pad_cannot_activate_from_below() {
        // Pad top at y=1.0. Player below on a floor (feet at y=0.0).
        let pos = Vec3::new(0.0, 0.9, 0.0); // feet 0.0
        let prev = Vec3::new(0.0, 0.9, 0.0);
        assert!(!crossed_platform_top(
            pos,
            prev,
            HE,
            Vec3::new(0.0, 0.825, 0.0),
            PAD_HE,
            1.0,
            TOP_TOLERANCE,
            0.0
        ));
    }

    #[test]
    fn launch_pad_cannot_activate_from_side() {
        let pos = Vec3::new(2.0, 1.9, 0.0); // no x footprint overlap
        let prev = Vec3::new(2.0, 1.9, 0.0);
        assert!(!crossed_platform_top(
            pos,
            prev,
            HE,
            Vec3::new(0.0, 0.825, 0.0),
            PAD_HE,
            1.0,
            TOP_TOLERANCE,
            0.0
        ));
    }

    #[test]
    fn launch_pad_fires_once_per_landing() {
        let mut mem = InteractionMemory::default();
        let key = player_key(5);
        let pos = Vec3::new(0.0, 1.9, 0.0); // feet on pad top (y=1.0)
        let prev = Vec3::new(0.0, 2.4, 0.0); // was above
        // Landing frame: touching -> entered.
        mem.begin_frame();
        assert!(crossed_platform_top(
            pos,
            prev,
            HE,
            Vec3::new(0.0, 0.825, 0.0),
            PAD_HE,
            1.0,
            TOP_TOLERANCE,
            -2.0
        ));
        mem.touch(key);
        assert!(mem.entered(key));
        // Standing frame: still touching, not entered.
        mem.begin_frame();
        assert!(crossed_platform_top(
            pos,
            pos,
            HE,
            Vec3::new(0.0, 0.825, 0.0),
            PAD_HE,
            1.0,
            TOP_TOLERANCE,
            0.0
        ));
        mem.touch(key);
        assert!(!mem.entered(key));
    }

    #[test]
    fn interaction_volumes_do_not_activate_through_floor_or_ceiling() {
        // Pad under a floor: player stands on the floor above, feet way above.
        let pos = Vec3::new(0.0, 4.9, 0.0);
        let prev = Vec3::new(0.0, 4.9, 0.0);
        assert!(!crossed_platform_top(
            pos,
            prev,
            HE,
            Vec3::new(0.0, 0.825, 0.0),
            PAD_HE,
            1.0,
            TOP_TOLERANCE,
            0.0
        ));
        // Rising beneath an object: velocity up -> no activation.
        let pos = Vec3::new(0.0, 0.5, 0.0);
        let prev = Vec3::new(0.0, 0.3, 0.0);
        assert!(!crossed_platform_top(
            pos,
            prev,
            HE,
            Vec3::new(0.0, 0.825, 0.0),
            PAD_HE,
            1.0,
            TOP_TOLERANCE,
            2.0
        ));
    }

    #[test]
    fn timed_gate_waits_for_doorway_to_clear() {
        let gate_center = Vec3::new(4.0, 2.0, 0.0);
        let gate_he = Vec3::new(0.55, 1.2, 0.3);
        // Body standing in the doorway blocks the close.
        assert!(gateway_blocked(
            vec![(Vec3::new(4.0, 2.0, 0.0), HE)],
            gate_center,
            gate_he
        ));
        // After moving away it no longer blocks.
        assert!(!gateway_blocked(
            vec![(Vec3::new(8.0, 2.0, 0.0), HE)],
            gate_center,
            gate_he
        ));
    }

    #[test]
    fn full_armor_does_not_consume_heal_pickup() {
        assert!(!heal_allowed(3));
        assert!(heal_allowed(2));
        assert!(heal_allowed(0));
    }

    #[test]
    fn prowler_damage_consumes_armor_before_respawn() {
        let (armor, alive) = armor_hit(2, 1);
        assert_eq!((armor, alive), (1, true));
        let (armor, alive) = armor_hit(1, 1);
        assert_eq!((armor, alive), (0, true));
        let (armor, alive) = armor_hit(0, 1);
        assert_eq!((armor, alive), (0, false));
        let (armor, alive) = armor_hit(3, 2);
        assert_eq!((armor, alive), (1, true));
    }

    #[test]
    fn thrown_impacts_dedupe_to_one_event() {
        let mut requests = DamageRequests::default();
        let target = Entity::from_raw_u32(77).unwrap();
        let source = Entity::from_raw_u32(1).unwrap();
        // Two impacts from the same collision source frame.
        requests.push(DamageRequest {
            target,
            source,
            amount: 1,
            attack: Some(AttackKind::ThrownImpact),
        });
        requests.push(DamageRequest {
            target,
            source,
            amount: 1,
            attack: Some(AttackKind::ThrownImpact),
        });
        let unique = requests.unique();
        assert_eq!(unique.len(), 1);
    }

    #[test]
    fn fan_force_is_capped_when_overlapping_multiple_fans() {
        // Two overlapping fans stack to 36, well above the 30 cap.
        let from_two_fans = Vec3::new(36.0, 0.0, 0.0);
        let capped = cap_fan_force(from_two_fans, MAX_FAN_FORCE);
        assert!(capped.length() <= MAX_FAN_FORCE + 1e-4);
        assert!((capped.length() - MAX_FAN_FORCE).abs() < 1e-3);
        // Single weak fan under the cap passes through unchanged.
        let weak = Vec3::new(6.0, 0.0, 0.0);
        assert_eq!(cap_fan_force(weak, MAX_FAN_FORCE), weak);
    }

    #[test]
    fn forced_motion_wins_by_priority() {
        let mut reqs = ForcedMotionRequests::default();
        reqs.requests.push(ForcedMotion {
            source: player_key(1),
            priority: PRIORITY_LAUNCH,
            position: None,
            velocity: Some(Vec3::X),
            control_lock: 0.9,
            respawn: false,
        });
        reqs.requests.push(ForcedMotion {
            source: player_key(2),
            priority: PRIORITY_TELEPORT,
            position: Some(Vec3::ZERO),
            velocity: Some(Vec3::ZERO),
            control_lock: 0.15,
            respawn: false,
        });
        let winner = reqs.winner().unwrap();
        assert_eq!(winner.priority, PRIORITY_TELEPORT);
        assert!(winner.position.is_some());
    }

    #[test]
    fn teleporter_does_not_bounce_until_player_exits() {
        let mut memory = InteractionMemory::default();
        memory.teleport_exit_lock = Some(player_key(7));
        // Still inside the destination volume: the lock holds.
        update_teleport_exit_lock(&mut memory, true);
        assert_eq!(memory.teleport_exit_lock, Some(player_key(7)));
        // Once the player exits, the lock clears and entry can fire again.
        update_teleport_exit_lock(&mut memory, false);
        assert_eq!(memory.teleport_exit_lock, None);
    }

    #[test]
    fn teleport_destination_is_deterministic_regardless_of_order() {
        let endpoints_a = vec![(30, Vec3::X), (10, Vec3::Y), (20, Vec3::Z)];
        let endpoints_b = vec![(10, Vec3::Y), (20, Vec3::Z), (30, Vec3::X)];
        for from in [10u32, 20, 30] {
            let da = teleport_destination(&endpoints_a, from).copied();
            let db = teleport_destination(&endpoints_b, from).copied();
            assert_eq!(da, db);
        }
        // From 30 the destination wraps to the lowest id (10).
        assert_eq!(
            teleport_destination(&endpoints_a, 30).map(|e| e.0),
            Some(10)
        );
        // Unpaired links have no destination.
        assert_eq!(teleport_destination(&[(1, Vec3::X)], 1), None);
    }

    #[test]
    fn speed_ring_contact_cannot_suppress_bumper_or_teleporter() {
        // A speed ring only boosts the player and never issues forced motion,
        // so the bumper / teleporter arbitration is unaffected by it.
        let mut reqs = ForcedMotionRequests::default();
        reqs.requests.push(ForcedMotion {
            source: player_key(1),
            priority: PRIORITY_BUMPER,
            position: None,
            velocity: Some(Vec3::Y * 12.0),
            control_lock: 0.3,
            respawn: false,
        });
        reqs.requests.push(ForcedMotion {
            source: player_key(2),
            priority: PRIORITY_TELEPORT,
            position: Some(Vec3::X),
            velocity: Some(Vec3::ZERO),
            control_lock: 0.15,
            respawn: false,
        });
        let winner = reqs.winner().unwrap();
        assert_eq!(winner.source, player_key(2));
        assert_eq!(winner.priority, PRIORITY_TELEPORT);
    }

    #[test]
    fn onoff_switch_toggles_once_per_touch_through_the_detect_frame() {
        let mut sim = Sim::with_default_step();
        let level = LevelDocument::default();
        let player = spawn_player(&mut sim.world, &level);
        sim.world.insert_resource(MakerMode::Play);
        sim.world.insert_resource(Paused(false));
        sim.world.insert_resource(MakerUi::default());
        sim.world.insert_resource(InteractionMemory::default());
        sim.world.insert_resource(ForcedMotionRequests::default());
        sim.world.insert_resource(OnOffState::default());
        let spawn = sim
            .world
            .get::<PlayerTransform>(player)
            .unwrap()
            .translation;
        sim.world.spawn((
            LevelEnt {
                id: 1,
                kind: EntityKind::OnOffSwitch,
            },
            Transform::from_translation(spawn),
            OnOffSwitch,
        ));

        let frame = |sim: &mut Sim| {
            begin_interaction_frame(&mut sim.world, false);
            detect_contacts(&mut sim.world);
            touch_onoff_switches(&mut sim.world);
        };

        frame(&mut sim);
        assert!(
            !sim.world.resource::<OnOffState>().on,
            "first touch toggles"
        );
        assert_eq!(sim.world.resource::<MakerUi>().status, "On/Off: OFF");

        frame(&mut sim);
        frame(&mut sim);
        assert!(
            !sim.world.resource::<OnOffState>().on,
            "standing on the switch must not toggle every frame"
        );

        sim.world
            .get_mut::<PlayerTransform>(player)
            .unwrap()
            .translation = spawn + Vec3::X * 5.0;
        frame(&mut sim);
        assert!(
            !sim.world.resource::<OnOffState>().on,
            "leaving does not toggle"
        );

        sim.world
            .get_mut::<PlayerTransform>(player)
            .unwrap()
            .translation = spawn;
        frame(&mut sim);
        assert!(
            sim.world.resource::<OnOffState>().on,
            "re-entry toggles again"
        );
        assert_eq!(sim.world.resource::<MakerUi>().status, "On/Off: ON");
    }
}
