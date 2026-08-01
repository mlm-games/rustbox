use bevy::prelude::*;

use rustbox_format::file::{FORMAT_VERSION, LevelFile};
use rustbox_format::level::LevelData;

use super::commands::CommandHistory;
use super::level::LevelDocument;
use super::mode::MakerMode;

pub const AUTOSAVE_KEY: &str = "level_autosave";
pub const COLLECTION_PREFIX: &str = "__col_";

/// Keys starting with "__" are internal (campaign progress, etc.) shouldn't
/// show up as player level slots.
pub fn list_slots(storage: &LevelStorage) -> Vec<String> {
    storage
        .0
        .list()
        .unwrap_or_default()
        .into_iter()
        .filter(|k| !k.starts_with("__"))
        .collect()
}

pub fn list_collection(storage: &LevelStorage) -> Vec<String> {
    storage
        .0
        .list()
        .unwrap_or_default()
        .into_iter()
        .filter(|k| k.starts_with(COLLECTION_PREFIX))
        .collect()
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
        .collect();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{COLLECTION_PREFIX}{}_{}", safe, secs)
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

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    pub struct FsBackend {
        dir: PathBuf,
    }

    impl FsBackend {
        pub fn new() -> Self {
            let dir = directories::ProjectDirs::from("com", "mlm-games", "ecosystem-template")
                .map(|d| d.data_dir().join("levels"))
                .unwrap_or_else(|| PathBuf::from("levels"));
            let _ = fs::create_dir_all(&dir);
            Self { dir }
        }

        fn path(&self, key: &str) -> PathBuf {
            let safe: String = key
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            self.dir.join(format!("{safe}.ron"))
        }
    }

    impl StorageBackend for FsBackend {
        fn save(&self, key: &str, data: &str) -> anyhow::Result<()> {
            let path = self.path(key);
            if path.exists() {
                let _ = fs::copy(&path, path.with_extension("ron.bak"));
            }
            fs::write(&path, data)?;
            Ok(())
        }

        fn load(&self, key: &str) -> anyhow::Result<Option<String>> {
            let path = self.path(key);
            if !path.exists() {
                let bak = path.with_extension("ron.bak");
                if bak.exists() {
                    return Ok(Some(fs::read_to_string(bak)?));
                }
                return Ok(None);
            }
            Ok(Some(fs::read_to_string(path)?))
        }

        fn list(&self) -> anyhow::Result<Vec<String>> {
            let mut out = vec![];
            for entry in fs::read_dir(&self.dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "ron")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    out.push(stem.to_string());
                }
            }
            out.sort();
            Ok(out)
        }

        fn delete(&self, key: &str) -> anyhow::Result<()> {
            let path = self.path(key);
            if path.exists() {
                fs::remove_file(path)?;
            }
            Ok(())
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;

    const PREFIX: &str = "maker3d:";

    pub struct LocalStorageBackend;

    fn storage() -> anyhow::Result<web_sys::Storage> {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .ok_or_else(|| anyhow::anyhow!("localStorage unavailable"))
    }

    impl StorageBackend for LocalStorageBackend {
        fn save(&self, key: &str, data: &str) -> anyhow::Result<()> {
            storage()?
                .set_item(&format!("{PREFIX}{key}"), data)
                .map_err(|_| anyhow::anyhow!("localStorage write failed (quota?)"))
        }

        fn load(&self, key: &str) -> anyhow::Result<Option<String>> {
            Ok(storage()?
                .get_item(&format!("{PREFIX}{key}"))
                .map_err(|_| anyhow::anyhow!("localStorage read failed"))?)
        }

        fn list(&self) -> anyhow::Result<Vec<String>> {
            let s = storage()?;
            let mut out = vec![];
            let len = s.length().unwrap_or(0);
            for i in 0..len {
                if let Ok(Some(k)) = s.key(i) {
                    if let Some(stripped) = k.strip_prefix(PREFIX) {
                        out.push(stripped.to_string());
                    }
                }
            }
            out.sort();
            Ok(out)
        }

        fn delete(&self, key: &str) -> anyhow::Result<()> {
            storage()?
                .remove_item(&format!("{PREFIX}{key}"))
                .map_err(|_| anyhow::anyhow!("localStorage delete failed"))
        }
    }
}

fn create_backend() -> Box<dyn StorageBackend> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Box::new(native::FsBackend::new())
    }
    #[cfg(target_arch = "wasm32")]
    {
        Box::new(web::LocalStorageBackend)
    }
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
    let file: LevelFile = ron::from_str(text)?;
    match file.version {
        1 | 2 => Ok(file.level),
        v => anyhow::bail!("unknown level format version {v}"),
    }
}

pub use rustbox_format::file::export_code as export_level_code;
pub use rustbox_format::file::import_code as import_level_code;

pub fn save_level(
    storage: &LevelStorage,
    level: &mut LevelDocument,
    key: &str,
) -> anyhow::Result<()> {
    level.rebuild_blocks_vec();
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
