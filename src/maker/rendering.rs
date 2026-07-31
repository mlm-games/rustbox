use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::MakerCleanup;
use super::block::BlockKind;
use super::chunk::CHUNK_SIZE;
use super::level::LevelDocument;
use super::player;

#[derive(Resource)]
pub struct MakerAssets {
    pub cube: Handle<Mesh>,
    pub chunk_material: Handle<StandardMaterial>,
    pub player_mesh: Handle<Mesh>,
    pub player_mat: Handle<StandardMaterial>,
    pub preview_mat: Handle<StandardMaterial>,
    pub ghost_mats: HashMap<BlockKind, Handle<StandardMaterial>>,
}

#[derive(Resource, Default)]
pub struct ChunkEntities(pub HashMap<IVec3, Entity>);

#[derive(Component)]
pub struct PlacementPreview;

#[derive(Component)]
pub struct GhostTimer(pub f32);

const FACES: [(IVec3, Vec3, Vec3, Vec3); 6] = [
    (IVec3::X, Vec3::new(1., 0., 0.), Vec3::Y, Vec3::Z),
    (IVec3::NEG_X, Vec3::new(0., 0., 0.), Vec3::Z, Vec3::Y),
    (IVec3::Y, Vec3::new(0., 1., 0.), Vec3::Z, Vec3::X),
    (IVec3::NEG_Y, Vec3::new(0., 0., 0.), Vec3::X, Vec3::Z),
    (IVec3::Z, Vec3::new(0., 0., 1.), Vec3::X, Vec3::Y),
    (IVec3::NEG_Z, Vec3::new(0., 0., 0.), Vec3::Y, Vec3::X),
];

fn build_chunk_mesh(level: &LevelDocument, cpos: IVec3) -> Option<Mesh> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let origin = cpos * CHUNK_SIZE;

    for lx in 0..CHUNK_SIZE {
        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let cell = origin + IVec3::new(lx, ly, lz);
                let Some(kind) = level.get_block(cell) else {
                    continue;
                };
                let color = kind.color().to_linear().to_f32_array();
                let base_world = cell.as_vec3();

                for (offset, corner, t1, t2) in FACES {
                    if level.get_block(cell + offset).is_some() {
                        continue;
                    }

                    let normal = t1.cross(t2);
                    let i0 = positions.len() as u32;
                    for p in [corner, corner + t1, corner + t1 + t2, corner + t2] {
                        let world = base_world + p;
                        positions.push(world.to_array());
                        normals.push(normal.to_array());
                        colors.push(color);
                    }
                    indices.extend_from_slice(&[i0, i0 + 1, i0 + 2, i0 + 2, i0 + 3, i0]);
                }
            }
        }
    }

    if indices.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

pub fn rebuild_dirty_chunks(
    mut commands: Commands,
    mut level: ResMut<LevelDocument>,
    assets: Option<Res<MakerAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: ResMut<ChunkEntities>,
) {
    let Some(assets) = assets else {
        return;
    };
    if level.dirty_chunks.is_empty() && !level.is_changed() {
        return;
    }

    let dirty: Vec<IVec3> = level.dirty_chunks.drain().collect();

    for cpos in dirty {
        match build_chunk_mesh(&level, cpos) {
            Some(mesh) => {
                let handle = meshes.add(mesh);
                match chunks.0.get(&cpos) {
                    Some(&e) => {
                        commands.entity(e).insert(Mesh3d(handle));
                    }
                    None => {
                        let e = commands
                            .spawn((
                                Mesh3d(handle),
                                MeshMaterial3d(assets.chunk_material.clone()),
                                Transform::IDENTITY,
                                MakerCleanup,
                            ))
                            .id();
                        chunks.0.insert(cpos, e);
                    }
                }
            }
            None => {
                if let Some(e) = chunks.0.remove(&cpos) {
                    commands.entity(e).despawn();
                }
            }
        }
    }

    chunks.0.retain(|cpos, e| {
        let origin = *cpos * CHUNK_SIZE;
        let has_content = level.map.keys().any(|k| {
            let d = *k - origin;
            (0..CHUNK_SIZE).contains(&d.x)
                && (0..CHUNK_SIZE).contains(&d.y)
                && (0..CHUNK_SIZE).contains(&d.z)
        });
        if !has_content {
            commands.entity(*e).despawn();
        }
        has_content
    });
}

pub fn spawn_place_ghost(
    commands: &mut Commands,
    assets: &MakerAssets,
    cell: IVec3,
    kind: BlockKind,
) {
    let e = commands
        .spawn((
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.ghost_mats[&kind].clone()),
            Transform::from_translation(cell.as_vec3() + Vec3::splat(0.5))
                .with_scale(Vec3::splat(1.04)),
            GhostTimer(0.25),
            MakerCleanup,
        ))
        .id();
    game_utils_bevy::juice::Juice::pop_in(commands, e, 0.18);
}

pub fn tick_ghosts(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut GhostTimer)>,
) {
    for (e, mut t) in &mut q {
        t.0 -= time.delta_secs();
        if t.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

pub fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    level: Res<LevelDocument>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    let mut chunk_mat = StandardMaterial::from_color(Color::WHITE);
    chunk_mat.perceptual_roughness = 0.9;
    let chunk_material = materials.add(chunk_mat);

    let player_mesh = meshes.add(Capsule3d::new(0.3, 1.2));
    let player_mat = materials.add(StandardMaterial::from_color(Color::srgb(0.9, 0.3, 0.3)));

    let mut preview = StandardMaterial::from_color(Color::srgba(1.0, 1.0, 1.0, 0.35));
    preview.alpha_mode = AlphaMode::Blend;
    let preview_mat = materials.add(preview);

    let mut ghost_mats = HashMap::new();
    for kind in [
        BlockKind::Grass,
        BlockKind::Stone,
        BlockKind::Hazard,
        BlockKind::Goal,
        BlockKind::Spawn,
    ] {
        ghost_mats.insert(
            kind,
            materials.add(StandardMaterial::from_color(kind.color())),
        );
    }

    let assets = MakerAssets {
        cube: cube.clone(),
        chunk_material,
        player_mesh: player_mesh.clone(),
        player_mat: player_mat.clone(),
        preview_mat: preview_mat.clone(),
        ghost_mats,
    };

    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(assets.preview_mat.clone()),
        Transform::from_scale(Vec3::splat(1.02)),
        Visibility::Hidden,
        PlacementPreview,
        MakerCleanup,
    ));

    player::spawn_player(&mut commands, &assets, &level);

    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 250.0,
        ..default()
    });

    commands.insert_resource(assets);
}
