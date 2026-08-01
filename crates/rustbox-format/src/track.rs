use serde::{Deserialize, Serialize};

pub type TrackId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackMode {
    #[default]
    PingPong,
    Loop,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackData {
    pub id: TrackId,
    pub points: Vec<[i32; 3]>,
    #[serde(default)]
    pub mode: TrackMode,
    #[serde(default = "default_speed")]
    pub speed: f32,
}

fn default_speed() -> f32 {
    2.0
}
