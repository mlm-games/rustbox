use bevy::prelude::*;

use rustbox_format::file::{FORMAT_VERSION, LevelFile, LevelFileV3, upgrade_v3};
use rustbox_format::level::LevelData;

use super::commands::CommandHistory;
use super::level::LevelDocument;
use super::mode::MakerMode;

pub const AUTOSAVE_KEY: &str = "level_autosave";
pub const COLLECTION_PREFIX: &str = "__col_";

/// Keys starting with "__" are internal (campaign progress, etc.) shouldn't
/// show up as player level slots.
pub fn list_slots(storage: &LevelStorage) -> Vec<String> {
    match storage.0.list() {
        Ok(v) => v.into_iter().filter(|k| !k.starts_with("__")).collect(),
        Err(e) => {
            bevy::log::warn!("Failed to list level slots: {e}");
            Vec::new()
        }
    }
}

pub fn list_collection(storage: &LevelStorage) -> Vec<String> {
    match storage.0.list() {
        Ok(v) => v
            .into_iter()
            .filter(|k| k.starts_with(COLLECTION_PREFIX))
            .collect(),
        Err(e) => {
            bevy::log::warn!("Failed to list collection: {e}");
            Vec::new()
        }
    }
}

fn collection_key(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{COLLECTION_PREFIX}{}_{}_{}",
        safe,
        nanos,
        std::process::id()
    )
}

/// Saves the current level into the browsable collection and returns the key.
/// The copy gets a fresh `created_at` timestamp and keeps the level's own name.
pub fn save_to_collection(
    storage: &LevelStorage,
    level: &mut LevelDocument,
) -> anyhow::Result<String> {
    if level.data.created_at == 0 {
        level.data.created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }
    let key = collection_key(&level.data.name);
    save_level(storage, level, &key)?;
    Ok(key)
}

pub fn delete_collection(storage: &LevelStorage, key: &str) -> anyhow::Result<()> {
    storage.0.delete(key)
}

pub trait StorageBackend: Send + Sync + 'static {
    fn save(&self, key: &str, data: &str) -> anyhow::Result<()>;
    fn load(&self, key: &str) -> anyhow::Result<Option<String>>;
    fn list(&self) -> anyhow::Result<Vec<String>>;
    fn delete(&self, key: &str) -> anyhow::Result<()>;
}

#[derive(Resource)]
pub struct LevelStorage(pub Box<dyn StorageBackend>);

impl Default for LevelStorage {
    fn default() -> Self {
        Self(create_backend())
    }
}

impl LevelStorage {
    /// In-memory backend for hermetic tests (no filesystem access).
    #[cfg(test)]
    pub fn for_testing() -> Self {
        Self(Box::new(SaveStoreBackend::<
            game_utils::storage::MemoryStorage,
        >::new_for_testing()))
    }
}

fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn levels_dir() -> std::path::PathBuf {
    if let Some(proj) = directories::ProjectDirs::from("com", "mlm-games", "rustbox") {
        let dir = proj.data_dir().join("levels");
        if std::fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    let dir = std::env::temp_dir().join("com-mlm-games-rustbox-levels");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Crash-safe backend built on `game_utils::save_store::SaveStore`:
/// temp+rename + fsync, throttled `.bak` rotation, corrupt quarantine, and
/// load recovery (temp → bak) instead of the previous ad-hoc copy/bak logic.
/// `FsStorage` already routes to OPFS on wasm, so one impl covers both.
pub struct SaveStoreBackend<S: game_utils::storage::Storage = game_utils::storage::FsStorage> {
    dir: std::path::PathBuf,
    storage: S,
}

impl SaveStoreBackend<game_utils::storage::FsStorage> {
    pub fn new() -> Self {
        Self {
            dir: levels_dir(),
            storage: game_utils::storage::FsStorage,
        }
    }
}

#[cfg(test)]
impl SaveStoreBackend<game_utils::storage::MemoryStorage> {
    pub fn new_for_testing() -> Self {
        Self {
            dir: std::path::PathBuf::from("/tmp/rustbox-levels-test"),
            storage: game_utils::storage::MemoryStorage::new(),
        }
    }
}

impl<S: game_utils::storage::Storage> SaveStoreBackend<S> {
    fn file_name(&self, key: &str) -> String {
        format!("{}.ron", sanitize_key(key))
    }

    fn store(&self, key: &str) -> game_utils::save_store::SaveStore<S> {
        game_utils::save_store::SaveStore::new_with_storage(
            self.dir.clone(),
            self.file_name(key),
            self.storage.clone(),
        )
        .with_validator(game_utils::save_store::SaveStore::<S>::is_intact_ron)
    }
}

impl<S: game_utils::storage::Storage> StorageBackend for SaveStoreBackend<S> {
    fn save(&self, key: &str, data: &str) -> anyhow::Result<()> {
        self.store(key)
            .write(data.as_bytes())
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    fn load(&self, key: &str) -> anyhow::Result<Option<String>> {
        use game_utils::save_store::LoadStatus;
        let store = self.store(key);
        let res = store.load(&game_utils::save_store::SaveStore::<S>::is_intact_ron, &[]);
        match res.status {
            LoadStatus::Ok => res
                .data
                .map(|b| String::from_utf8(b).map_err(|e| anyhow::anyhow!("{e}")))
                .transpose(),
            LoadStatus::Missing => Ok(None),
            LoadStatus::Corrupt => match res.data {
                Some(b) => Ok(Some(
                    String::from_utf8(b).map_err(|e| anyhow::anyhow!("{e}"))?,
                )),
                None => anyhow::bail!("saved level is corrupt (quarantined)"),
            },
            LoadStatus::Unreadable => anyhow::bail!("saved level is unreadable (locked?)"),
        }
    }

    fn list(&self) -> anyhow::Result<Vec<String>> {
        use game_utils::storage::Storage;
        let entries = self.storage.read_dir(&self.dir).unwrap_or_default();
        let mut out = vec![];
        for path in entries {
            if path.extension().is_some_and(|e| e == "ron")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                if stem.starts_with("temp_") || stem.starts_with("corrupted_") {
                    continue;
                }
                out.push(stem.to_string());
            }
        }
        out.sort();
        Ok(out)
    }

    fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.store(key).delete();
        Ok(())
    }
}

fn create_backend() -> Box<dyn StorageBackend> {
    Box::new(SaveStoreBackend::new())
}

pub fn serialize_level(level: &LevelData) -> anyhow::Result<String> {
    let file = LevelFile {
        version: FORMAT_VERSION,
        level: level.clone(),
    };
    Ok(ron::ser::to_string_pretty(
        &file,
        ron::ser::PrettyConfig::default(),
    )?)
}

pub fn deserialize_level(text: &str) -> anyhow::Result<LevelData> {
    #[derive(serde::Deserialize)]
    struct LevelHeader {
        version: u32,
    }
    let header: LevelHeader =
        ron::from_str(text).map_err(|_| anyhow::anyhow!("could not read level file"))?;
    let data = match header.version {
        4 | 5 | 6 | 7 | 8 => {
            let file: LevelFile =
                ron::from_str(text).map_err(|_| anyhow::anyhow!("corrupted level file"))?;
            file.level
        }
        1 | 2 | 3 => {
            let file: LevelFileV3 =
                ron::from_str(text).map_err(|_| anyhow::anyhow!("corrupted level file"))?;
            upgrade_v3(file.level)
        }
        v => anyhow::bail!("unknown level format version {v}"),
    };
    rustbox_format::file::validate_level(&data)
        .map_err(|e| anyhow::anyhow!("invalid level: {e}"))?;
    Ok(data)
}

pub use rustbox_format::file::export_code as export_level_code;
use rustbox_format::file::import_code as import_code_raw;

/// Share-code import: validates like every other untrusted path and strips
/// forged verification/record flags so codes can't grant publish rights.
pub fn import_level_code(code: &str) -> anyhow::Result<LevelData> {
    let mut data = import_code_raw(code.trim()).map_err(|e| anyhow::anyhow!("{e}"))?;
    rustbox_format::file::validate_level(&data)
        .map_err(|e| anyhow::anyhow!("invalid level: {e}"))?;
    data.is_verified = false;
    data.author_time = None;
    data.record_ms = None;
    Ok(data)
}

pub fn save_level(
    storage: &LevelStorage,
    level: &mut LevelDocument,
    key: &str,
) -> anyhow::Result<()> {
    if key.trim_start().starts_with("__") {
        anyhow::bail!("name is reserved");
    }
    level.rebuild_blocks_vec();
    rustbox_format::file::validate_level(&level.data)
        .map_err(|e| anyhow::anyhow!("invalid level: {e}"))?;
    let text = serialize_level(&level.data)?;
    storage.0.save(key, &text)
}

pub fn apply_level_data(level: &mut LevelDocument, history: &mut CommandHistory, data: LevelData) {
    level.replace_data(data);

    history.undo.clear();
    history.redo.clear();
}

pub fn load_level(
    storage: &LevelStorage,
    level: &mut LevelDocument,
    history: &mut CommandHistory,
    key: &str,
) -> anyhow::Result<bool> {
    let Some(text) = storage.0.load(key)? else {
        return Ok(false);
    };
    let data = deserialize_level(&text)?;
    apply_level_data(level, history, data);
    Ok(true)
}

pub fn save_load_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<super::mode::InputCapture>,
    storage: Res<LevelStorage>,
    mode: Res<MakerMode>,
    mut level: ResMut<LevelDocument>,
    mut history: ResMut<CommandHistory>,
) {
    if capture.ui_wants_keyboard {
        return;
    }
    if *mode != MakerMode::Edit {
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !ctrl {
        return;
    }

    if keys.just_pressed(KeyCode::KeyS) {
        match save_level(&storage, &mut level, AUTOSAVE_KEY) {
            Ok(()) => info!("Level saved"),
            Err(e) => error!("Save failed: {e}"),
        }
    }
    if keys.just_pressed(KeyCode::KeyL) {
        match load_level(&storage, &mut level, &mut history, AUTOSAVE_KEY) {
            Ok(true) => info!("Level loaded"),
            Ok(false) => warn!("No saved level found"),
            Err(e) => error!("Load failed: {e}"),
        }
    }
}
