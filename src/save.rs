use bevy::prelude::*;
use game_utils::save::Versioned;
use serde::{Deserialize, Serialize};

pub const SAVE_VERSION: u32 = 1;

fn clamp01<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = f32::deserialize(deserializer)?;
    if v.is_finite() {
        Ok(v.clamp(0.0, 1.0))
    } else {
        Ok(1.0)
    }
}

#[allow(dead_code)]
fn clamp01_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct SaveData {
    #[serde(default)]
    pub version: u32,
    pub high_score: u32,
    pub settings: SettingsData,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SettingsData {
    #[serde(deserialize_with = "clamp01", default = "default_master")]
    pub master_volume: f32,
    #[serde(deserialize_with = "clamp01", default = "default_sfx")]
    pub sfx_volume: f32,
    #[serde(deserialize_with = "clamp01", default = "default_music")]
    pub music_volume: f32,
    pub language: String,
}

fn default_master() -> f32 { 1.0 }
fn default_sfx() -> f32 { 1.0 }
fn default_music() -> f32 { 0.8 }

impl Default for SettingsData {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            sfx_volume: 1.0,
            music_volume: 0.8,
            language: "en".to_string(),
        }
    }
}

impl SettingsData {
    pub fn sanitized(mut self) -> Self {
        self.master_volume = clamp01_or(self.master_volume, 1.0);
        self.sfx_volume = clamp01_or(self.sfx_volume, 1.0);
        self.music_volume = clamp01_or(self.music_volume, 0.8);
        if self.language.trim().is_empty() {
            self.language = "en".to_string();
        }
        self
    }
}

impl Default for SaveData {
    fn default() -> Self {
        Self {
            version: SAVE_VERSION,
            high_score: 0,
            settings: SettingsData::default(),
        }
    }
}

impl SaveData {
    pub fn sanitized(mut self) -> Self {
        self.settings = self.settings.sanitized();
        self
    }
}

impl Versioned for SaveData {
    fn version(&self) -> u32 {
        self.version
    }

    fn set_version(&mut self, version: u32) {
        self.version = version;
    }
}
