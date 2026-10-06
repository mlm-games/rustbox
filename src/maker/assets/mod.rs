//! Runtime model assets: the shipped manifests decide which glTF replaces a
//! kind's placeholder geometry, and each model is imported on first use.
//!
//! The original resolved the same tables through Bevy's `AssetServer`
//! (`asset_manifest.rs`, `block_asset_manifest.rs`, `rendering::setup_world`),
//! which is why entity kinds and block kind x shape pairs stay data-only
//! swaps here. Import failures are never silent: the pair keeps its procedural
//! geometry and the reason is logged once per path.

mod pack;

use std::collections::HashMap;
use std::rc::Rc;

use glam::{Mat3, Mat4, Quat, Vec3};
use repame_view3d::{
    Animation, BatchDesc, MeshGroup, Rgb, SceneUpload, Skeleton, SkeletonLoop, SkeletonPlayer,
    SkinnedMesh, TextureImage, decode_slice_images, import_animations, import_skeleton,
    import_skinned, import_slice_textured,
};
use rustbox_format::{ALL_BLOCK_KINDS, ALL_BLOCK_SHAPES, ALL_ENTITY_KINDS, BlockKind, BlockShape};
use serde::Deserialize;

use super::entity_data::EntityKind;

/// Texture pages are 512px: the shared entity colormap is 512x512 and the
/// character atlases are 32x32, so no page is downscaled or half empty.
const LAYER_SIZE: u32 = 512;

/// Array layers reserved for the whole session. Fixed on purpose: the engine
/// rebuilds (and empties) the batch's texture array whenever its shape
/// changes, and each page is uploaded exactly once, so a `layers` count that
/// grew with the imports would blank every page uploaded before the change.
/// The pack needs two (shared entity colormap, player atlas); four leaves
/// headroom.
const TEXTURE_LAYERS: u32 = 4;

/// How a loaded model is colored. Matches the manifest's `tint` field.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum TintMode {
    /// Keep the pack albedo.
    #[default]
    Model,
    /// Flat kind color.
    Kind,
    /// Flat On/Off channel color.
    Link,
}

/// One manifest row: which glTF to draw and how to place it. The manifests
/// also carry collider, preview and bounds fields this renderer does not need,
/// so serde skips them and only the visual placement is read here. RON names
/// the entity rows, so the type carries that name too.
#[derive(Clone, Deserialize)]
#[serde(rename = "EntityAssetEntry")]
struct ManifestEntry {
    model: Option<String>,
    #[serde(default = "unit")]
    scale: f32,
    #[serde(default)]
    y_offset: f32,
    #[serde(default)]
    tint: TintMode,
}

fn unit() -> f32 {
    1.0
}

/// The original loaded this scene directly instead of through a manifest.
const PLAYER_MODEL: &str = "models/cubeworld/Character_Male_2.gltf";

/// Struct names must match the manifest's, since RON names every value.
#[derive(Deserialize)]
struct EntityModelManifest {
    entities: HashMap<String, ManifestEntry>,
}

#[derive(Deserialize)]
struct BlockAssetManifest {
    blocks: HashMap<String, ManifestEntry>,
}

impl ModelAssets {
    /// Shape identity of the batch these pages belong to.
    fn desc_key(&self) -> (u32, u32) {
        (self.desc.layer_size, self.desc.layers)
    }
}

/// An imported model: world-space groups plus its manifest placement.
pub struct Template {
    pub groups: Vec<MeshGroup>,
    scale: f32,
    y_offset: f32,
    pub tint: TintMode,
}

impl Template {
    /// Model matrix for one instance, matching the original's spawn: rotate by
    /// yaw about the origin, scale, then translate to the position plus the
    /// manifest's vertical offset.
    pub fn matrix(&self, position: Vec3, yaw: f32) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            Quat::from_rotation_y(yaw),
            position + Vec3::Y * self.y_offset,
        )
    }

    /// Model matrix from an explicit transform (already-rotated entity pose).
    pub fn matrix_from(&self, transform: Mat4) -> Mat4 {
        transform * Mat4::from_scale_rotation_translation(
            Vec3::splat(self.scale),
            Quat::IDENTITY,
            Vec3::Y * self.y_offset,
        )
    }
}

/// Identical images across models share one array layer, keyed by content.
#[derive(Default)]
struct Pages {
    by_hash: HashMap<u64, u32>,
    /// Every page ever allocated, so a batch rebuild (which empties the array)
    /// can be answered by re-sending all of them.
    all: Vec<SceneUpload>,
    /// Pages not yet handed to a presented frame.
    pending: Vec<SceneUpload>,
    /// Batch shape the pending set was built against.
    /// `(layer_size, layers)` the pending set was built against; a change means
    /// the batch was rebuilt and its pages have to be re-sent.
    sent: Option<(u32, u32)>,
    live: u32,
}

impl Pages {
    fn hash(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Layer for one decoded image, deduplicated by content. `None` once the
    /// reserved array is full: the caller draws the model with its material
    /// tint instead of sampling a layer that does not exist.
    fn layer_for(&mut self, path: &str, image: usize, upload: SceneUpload) -> Option<u32> {
        let hash = Self::hash(&upload.rgba);
        if let Some(existing) = self.by_hash.get(&hash) {
            return Some(*existing);
        }
        let layer = self.live;
        if layer >= TEXTURE_LAYERS {
            log::error!(
                "assets: {path} image {image} needs layer {layer} but only {TEXTURE_LAYERS} are reserved; drawing untinted"
            );
            return None;
        }
        self.by_hash.insert(hash, layer);
        self.live += 1;
        let upload = SceneUpload { page: layer, ..upload };
        self.pending.push(upload.clone());
        self.all.push(upload);
        Some(layer)
    }
}

/// Imported models for the whole level, plus the batch texture array the
/// uploads feed.
pub struct ModelAssets {
    entity: HashMap<EntityKind, Option<ManifestEntry>>,
    block: HashMap<(BlockKind, BlockShape), Option<ManifestEntry>>,
    imported: HashMap<String, Option<Rc<Template>>>,
    characters: HashMap<String, Option<Rc<CharacterModel>>>,
    pages: Pages,
    desc: BatchDesc,
}

impl Default for ModelAssets {
    fn default() -> Self {
        Self::new()
    }
}

fn entity_table() -> HashMap<EntityKind, Option<ManifestEntry>> {
    let parsed = ron::from_str::<EntityModelManifest>(pack::ENTITY_MANIFEST).ok();
    let Some(manifest) = parsed else {
        log::error!("assets: entities.ron did not parse; drawing procedural shapes");
        return HashMap::new();
    };
    ALL_ENTITY_KINDS
        .iter()
        .map(|kind| {
            let key = format!("{kind:?}");
            (*kind, manifest.entities.get(&key).cloned())
        })
        .collect()
}

fn block_table() -> HashMap<(BlockKind, BlockShape), Option<ManifestEntry>> {
    let parsed = ron::from_str::<BlockAssetManifest>(pack::BLOCK_MANIFEST).ok();
    let Some(manifest) = parsed else {
        log::error!("assets: blocks.ron did not parse; drawing procedural voxel shapes");
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for kind in ALL_BLOCK_KINDS {
        for shape in ALL_BLOCK_SHAPES {
            let key = format!("{kind:?}/{shape:?}");
            out.insert((*kind, *shape), manifest.blocks.get(&key).cloned());
        }
    }
    out
}

impl ModelAssets {
    pub fn new() -> Self {
        Self {
            entity: entity_table(),
            block: block_table(),
            imported: HashMap::new(),
            characters: HashMap::new(),
            pages: Pages::default(),
            desc: BatchDesc {
                layer_size: LAYER_SIZE,
                layers: TEXTURE_LAYERS,
                ..BatchDesc::default()
            },
        }
    }

    /// The play-mode player model. The original loaded this scene directly
    /// (`models/cubeworld/Character_Male_2.gltf#Scene0`) rather than through a
    /// manifest.
    pub fn player(&mut self) -> Option<Rc<CharacterModel>> {
        self.character(PLAYER_MODEL)
    }

    /// True when a block pair is drawn from a pack model instead of the
    /// procedural voxel faces. Water never is: it stays a translucent
    /// volume, exactly as the original forced.
    pub fn block_is_model(&self, kind: BlockKind, shape: BlockShape) -> bool {
        kind != BlockKind::Water
            && self
                .block
                .get(&(kind, shape))
                .is_some_and(|entry| entry.as_ref().is_some_and(|e| e.model.is_some()))
    }

    /// Batch descriptor for the reserved texture array. Constant for the whole
    /// session: see [`TEXTURE_LAYERS`].
    pub fn desc(&self) -> BatchDesc {
        self.desc
    }

    /// Drain accumulated page uploads; the frame feeds each exactly once.
    /// Uploads the next presented frame must carry. Non-destructive: pages stay
    /// queued until [`Self::confirm_uploads`] runs for a frame that actually
    /// reached the screen, because frames built while the 3D view is not
    /// mounted are discarded and their uploads would never be applied.
    pub fn uploads(&self) -> Vec<SceneUpload> {
        if self.pages.sent != Some(self.desc_key()) {
            // The batch was rebuilt with a different shape (a resize or sample
            // count change empties its texture array), so re-send every page.
            return self.pages.all.clone();
        }
        self.pages.pending.clone()
    }

    /// Called once the frame carrying these uploads was mounted for display.
    pub fn confirm_uploads(&mut self) {
        self.pages.sent = Some(self.desc_key());
        self.pages.pending.clear();
    }

    fn bytes(path: &str) -> Option<&'static [u8]> {
        let hit = pack::PACK
            .binary_search_by(|(key, _)| (*key).cmp(path))
            .ok()?;
        Some(pack::PACK[hit].1)
    }

    fn import(&mut self, entry: &ManifestEntry) -> Option<Rc<Template>> {
        // Manifests name a glTF scene (`path#Scene0`); the pack is keyed by the
        // file, and every entry points at scene 0.
        let scene = entry.model.as_deref()?;
        let path = scene.split('#').next().unwrap_or(scene);
        if let Some(hit) = self.imported.get(path) {
            return hit.clone();
        }
        let template = Self::bytes(path).and_then(|bytes| {
            let textured = match import_slice_textured(bytes, LAYER_SIZE) {
                Ok(textured) => textured,
                Err(err) => {
                    log::error!("assets: {path} did not import: {err}");
                    return None;
                }
            };
            let mut meshes = textured.meshes;
            for mesh in &mut meshes {
                for group in &mut mesh.groups {
                    if group.uvs.is_empty() {
                        continue;
                    }
                    // `uploads` is sparse: a file whose earlier image failed to
                    // decode leaves holes, so resolve through the image map
                    // rather than indexing by page.
                    let placed = group
                        .base_image
                        .and_then(|image| textured.pages.get(&image))
                        .copied();
                    // No pixels for this image (untextured pack, or an image
                    // that failed to decode): drop the uvs so the group draws
                    // with its material tint instead of sampling nothing.
                    let page = placed
                        .and_then(|placed| textured.uploads.get(placed.page as usize))
                        .and_then(|upload| {
                            self.pages.layer_for(path, placed.unwrap().page as usize, upload.clone())
                        });
                    match page {
                        Some(page) => group.texture_page = page,
                        None => {
                            log::warn!("assets: {path} has uvs but no pixels; drawing flat");
                            group.uvs.clear();
                        }
                    }
                }
            }
            Some(Rc::new(Template {
                groups: meshes.into_iter().flat_map(|mesh| mesh.groups).collect(),
                scale: entry.scale,
                y_offset: entry.y_offset,
                tint: entry.tint,
            }))
        });
        self.imported.insert(path.to_string(), template.clone());
        template
    }

    /// Template for an entity kind, imported on first request.
    pub fn entity(&mut self, kind: EntityKind) -> Option<Rc<Template>> {
        let entry = self.entity.get(&kind)?.clone()?;
        self.import(&entry)
    }

    /// Template for a block kind and shape, imported on first request.
    pub fn block(&mut self, kind: BlockKind, shape: BlockShape) -> Option<Rc<Template>> {
        let entry = self.block.get(&(kind, shape))?.clone()?;
        self.import(&entry)
    }

    /// Import a skinned character (player's third-person model, the prowler)
    /// with its named clips. Memoized like the static templates.
    pub fn character(&mut self, path: &'static str) -> Option<Rc<CharacterModel>> {
        if let Some(hit) = self.characters.get(path) {
            return hit.clone();
        }
        let character = Self::bytes(path).and_then(|bytes| {
            let skinned = import_skinned(bytes)
                .map_err(|err| log::error!("assets: {path} skin import failed: {err}"))
                .ok()?;
            let skeleton =
                import_skeleton(bytes).map_err(|err| {
                    log::error!("assets: {path} skeleton import failed: {err}")
                }).ok()?;
            let animations = import_animations(bytes).map_err(|err| {
                log::error!("assets: {path} animation import failed: {err}")
            }).ok()?;
            let pages = self.pages_for(path, bytes);
            let mut meshes = skinned;
            for mesh in &mut meshes {
                let Some(&image) = mesh.base_image.as_ref() else {
                    continue;
                };
                match pages.get(&image) {
                    Some(page) => mesh.assign_page(page.layer, page.w, page.h, LAYER_SIZE),
                    None => {
                        log::warn!("assets: {path} skin has no pixels; drawing untextured");
                        mesh.uvs.clear();
                    }
                }
            }
            Some(Rc::new(CharacterModel {
                meshes,
                skeleton,
                clips: animations
                    .into_iter()
                    .map(|anim| (anim.name.clone(), anim))
                    .collect(),
                source: path,
            }))
        });
        self.characters
            .insert(path.to_string(), character.clone());
        character
    }

    /// Decode a model's images into deduplicated array layers, sized to the
    /// layer edge. Images larger than the layer are reported rather than
    /// resampled, so a mismatched atlas can never sample out of range.
    fn pages_for(&mut self, path: &str, bytes: &'static [u8]) -> HashMap<usize, PlacedImage> {
        let mut out = HashMap::new();
        let Ok(images) = decode_slice_images(bytes) else {
            log::warn!("assets: {path} images did not decode");
            return out;
        };
        for image in images {
            let TextureImage::Decoded {
                image,
                width,
                height,
                rgba,
            } = image
            else {
                continue;
            };
            if width > LAYER_SIZE || height > LAYER_SIZE {
                log::error!(
                    "assets: {path} image {width}x{height} exceeds the {LAYER_SIZE}px layer; skipped"
                );
                continue;
            }
            let upload = SceneUpload {
                page: 0,
                x: 0,
                y: 0,
                w: width,
                h: height,
                rgba,
            };
            let Some(layer) = self.pages.layer_for(path, image, upload) else {
                continue;
            };
            out.insert(
                image,
                PlacedImage {
                    layer,
                    w: width,
                    h: height,
                },
            );
        }
        out
    }
}

struct PlacedImage {
    layer: u32,
    w: u32,
    h: u32,
}

/// An animated character: bind-pose meshes, the skeleton they pose against,
/// and its named clips. Shared and immutable; playback state lives in
/// [`Playback`].
pub struct CharacterModel {
    meshes: Vec<SkinnedMesh>,
    skeleton: Skeleton,
    clips: Vec<(String, Animation)>,
    /// Pack path, for clip-miss diagnostics.
    source: &'static str,
}

impl CharacterModel {
    /// One looping player per clip, starting on the idle clip when the model
    /// has one (its first clip is whatever the file happens to list first).
    pub fn playback(&self) -> Playback {
        let mut playback = Playback {
            players: self
                .clips
                .iter()
                .map(|(_, anim)| SkeletonPlayer::new(anim, SkeletonLoop::Loop))
                .collect(),
            playing: None,
        };
        let start = self
            .clips
            .iter()
            .position(|(name, _)| name == "Idle")
            .or_else(|| (!self.clips.is_empty()).then_some(0));
        if let Some(index) = start {
            playback.playing = Some(index);
            playback.players[index].play();
        }
        playback
    }
}

/// Clip selection and the clock driving it.
pub struct Playback {
    players: Vec<SkeletonPlayer>,
    playing: Option<usize>,
}

impl Playback {
    /// Switch clips, restarting the new one. Matches the original's
    /// `play_if_needed`: an already-playing clip keeps its time.
    pub fn play(&mut self, model: &CharacterModel, name: &str) {
        let Some(index) = model.clips.iter().position(|(clip, _)| clip == name) else {
            log::warn!("assets: {} has no clip named {name}", model.source);
            return;
        };
        if self.playing == Some(index) {
            return;
        }
        let player = &mut self.players[index];
        player.seek(0.0);
        player.play();
        self.playing = Some(index);
    }

    pub fn advance(&mut self, dt: f32) {
        if let Some(index) = self.playing {
            self.players[index].advance(dt);
        }
    }

    /// Posed geometry for one instance, in world space. `xform` places and
    /// orients the character and is folded into the joint matrices, so the
    /// result is ready to push as an ordinary group.
    ///
    /// This is the CPU path on purpose: the batch frustum-culls skinned draws
    /// against the *bind-pose* vertex buffer, and the world transform of a
    /// character lives in the joint palette, so a GPU-skinned draw placed away
    /// from the origin gets culled. Posing on the CPU puts the vertices where
    /// they actually are, which culls and sorts correctly.
    pub fn posed(&self, model: &CharacterModel, xform: Mat4) -> Vec<MeshGroup> {
        let Some(index) = self.playing else {
            return Vec::new();
        };
        let player = &self.players[index];
        let (_, anim) = &model.clips[index];
        model
            .meshes
            .iter()
            .map(|mesh| {
                let joints: Vec<Mat4> = player
                    .joint_matrices(anim, &model.skeleton, mesh)
                    .into_iter()
                    .map(|joint| xform * joint)
                    .collect();
                mesh.pose(&joints)
            })
            .filter(|group| !group.is_empty())
            .collect()
    }
}

/// Multiply the colours [`push_instance`] just appended (from index `from`
/// onward) by a flat tint. The manifests use this for the `Kind` and `Link`
/// tint modes, where the original force-tinted the whole model so the kind /
/// channel colour language stays readable.
pub fn tint_instance(dst: &mut MeshGroup, from: usize, flat: Rgb) {
    for color in dst.colors.iter_mut().skip(from) {
        *color = [color[0] * flat[0], color[1] * flat[1], color[2] * flat[2]];
    }
}

/// Append `src`'s geometry transformed by `xform` into `dst`. Push style is
/// taken from `src` so a group never mixes lit/unlit or textured/untextured
/// verts (the batch drops mixed groups).
pub fn push_instance(dst: &mut MeshGroup, src: &MeshGroup, xform: Mat4) {
    if src.is_empty() {
        return;
    }
    debug_assert!(
        dst.positions.is_empty()
            || (dst.uvs.is_empty() == src.uvs.is_empty()
                && dst.normals.is_empty() == src.normals.is_empty()
                && dst.texture_page == src.texture_page),
        "push_instance: destination group style differs from the source"
    );
    dst.depth_test = src.depth_test;
    dst.transparent = src.transparent;
    dst.alpha = src.alpha;
    dst.alpha_cutoff = src.alpha_cutoff;
    dst.material = src.material;
    dst.texture_page = src.texture_page;
    dst.base_image = src.base_image;
    let base = dst.positions.len() as u32;
    // Inverse-transpose keeps normals correct under scale; a singular matrix
    // has no inverse, so fall back to the source normal rather than poisoning
    // the buffer with NaNs.
    let normal = Mat3::from_mat4(xform).inverse().transpose();
    for (index, position) in src.positions.iter().enumerate() {
        let point = xform.transform_point3(Vec3::from(*position));
        if !point.is_finite() {
            dst.positions.truncate(base as usize);
            dst.colors.truncate(base as usize);
            dst.normals.truncate(base as usize);
            dst.uvs.truncate(base as usize);
            return;
        }
        dst.positions.push(point.to_array());
        dst.colors.push(*src.colors.get(index).unwrap_or(&[1.0; 3]));
        if let Some(n) = src.normals.get(index) {
            let rotated = (normal * Vec3::from(*n)).try_normalize().unwrap_or(Vec3::Y);
            dst.normals.push(rotated.to_array());
        }
        if let Some(uv) = src.uvs.get(index) {
            dst.uvs.push(*uv);
        }
    }
    for index in &src.indices {
        dst.indices.push(base + index);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_layers_never_grow_and_pages_dedup() {
        let mut assets = ModelAssets::new();
        let before = assets.desc();
        // Entities: the whole catalog shares one colormap page.
        for kind in ALL_ENTITY_KINDS {
            assets.entity(*kind);
        }
        // Characters: one more page for the player atlas.
        assets.player();
        let after = assets.desc();
        assert_eq!(before.layers, after.layers, "layers must stay fixed");
        assert_eq!(after.layers, TEXTURE_LAYERS);
        let uploads = assets.uploads();
        assert_eq!(
            uploads.len(),
            2,
            "entity colormap and player atlas should dedup to two pages"
        );
        // The posed player must be a valid, world-placed group: that is the
        // path the frame draws, and its bounds are what frustum culling sees.
        let player = assets.player().expect("player model imported");
        let mut playback = player.playback();
        playback.play(&player, "Idle");
        let bind = playback.posed(&player, Mat4::IDENTITY);
        assert!(
            bind[0].bounds().is_some_and(|(min, _)| min[0].abs() < 2.0),
            "bind pose should sit near the origin"
        );
        let posed = playback.posed(&player, Mat4::from_translation(Vec3::new(20.0, 3.0, -8.0)));
        assert_eq!(posed.len(), 1, "one posed group for the player mesh");
        let (min, max) = posed[0].bounds().expect("posed group has bounds");
        let min = Vec3::from_array(min);
        let max = Vec3::from_array(max);
        assert!(
            min.x > 10.0 && min.z < 0.0,
            "posed geometry must sit at the model transform, not the origin: {min:?}..{max:?}"
        );
        assert!(max.x > min.x && max.z > min.z);
        assert!(
            posed[0].uvs.is_empty() || posed[0].texture_page < TEXTURE_LAYERS,
            "posed group must sample a reserved layer"
        );
        assert!(posed[0].positions.iter().all(|p| p[0].is_finite()));
        assert!(uploads.iter().all(|u| u.page < TEXTURE_LAYERS));
        assert!(uploads
            .iter()
            .all(|u| u.rgba.len() == (u.w * u.h * 4) as usize));
        // Pages stay queued until a frame actually presented them, so a frame
        // built while the 3D view is unmounted cannot lose them.
        assert_eq!(assets.uploads().len(), 2);
        assets.confirm_uploads();
        assert!(
            assets.uploads().is_empty(),
            "confirmed pages must not be re-sent every frame"
        );
    }

    #[test]
    fn overflowing_the_page_budget_reports_instead_of_silently_dropping() {
        let mut pages = Pages::default();
        let image = |byte: u8| SceneUpload {
            page: 0,
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            rgba: vec![byte; 4],
        };
        for index in 0..TEXTURE_LAYERS {
            assert!(pages.layer_for("test", index as usize, image(index as u8)).is_some());
        }
        assert!(
            pages.layer_for("test", 99, image(200)).is_none(),
            "the reserved array must not be overrun"
        );
        assert_eq!(pages.pending.len(), TEXTURE_LAYERS as usize);
        assert_eq!(pages.all.len(), TEXTURE_LAYERS as usize);
        // Identical content resolves to the layer already assigned.
        assert_eq!(
            pages.layer_for("test", 0, image(0)),
            Some(0),
            "identical pixels must share a layer"
        );
    }
}
