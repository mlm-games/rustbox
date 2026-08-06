use repose_core::prelude::Color as RColor;

use crate::maker::level::LevelTag;

/// Rustbox UI tokens - dark creator workspace.
pub mod tok {
    use super::RColor;

    pub fn bg_deep() -> RColor {
        RColor::from_rgba(6, 8, 14, 255)
    }
    pub fn bg_panel() -> RColor {
        RColor::from_rgba(14, 16, 24, 240)
    }
    pub fn bg_panel_solid() -> RColor {
        RColor::from_rgba(18, 20, 30, 255)
    }
    pub fn bg_elevated() -> RColor {
        RColor::from_rgba(28, 32, 46, 255)
    }
    pub fn bg_chip() -> RColor {
        RColor::from_rgba(40, 44, 62, 255)
    }
    pub fn bg_rail() -> RColor {
        RColor::from_rgba(12, 14, 22, 250)
    }
    pub fn bg_status() -> RColor {
        RColor::from_rgba(10, 12, 18, 245)
    }
    pub fn bg_modal() -> RColor {
        RColor::from_rgba(18, 20, 28, 255)
    }
    pub fn scrim() -> RColor {
        RColor::from_rgba(0, 0, 0, 180)
    }

    pub fn accent() -> RColor {
        RColor::from_rgba(88, 166, 255, 255)
    }
    pub fn accent_soft() -> RColor {
        RColor::from_rgba(56, 120, 200, 255)
    }
    pub fn create() -> RColor {
        RColor::from_rgba(72, 148, 255, 255)
    }
    pub fn play() -> RColor {
        RColor::from_rgba(52, 180, 110, 255)
    }
    pub fn danger() -> RColor {
        RColor::from_rgba(220, 72, 72, 255)
    }
    pub fn warn() -> RColor {
        RColor::from_rgba(240, 180, 64, 255)
    }
    pub fn ok() -> RColor {
        RColor::from_rgba(80, 200, 120, 255)
    }
    pub fn gold() -> RColor {
        RColor::from_rgba(217, 184, 115, 255)
    }

    pub fn text() -> RColor {
        RColor::from_rgba(236, 238, 245, 255)
    }
    pub fn text_dim() -> RColor {
        RColor::from_rgba(150, 154, 170, 255)
    }
    pub fn text_mute() -> RColor {
        RColor::from_rgba(110, 114, 130, 255)
    }
    pub fn outline() -> RColor {
        RColor::from_rgba(48, 52, 68, 255)
    }

    pub const R_SM: f32 = 8.0;
    pub const R_MD: f32 = 12.0;
    pub const R_LG: f32 = 16.0;
    pub const R_PILL: f32 = 20.0;
    pub const PAD: f32 = 12.0;
    pub const GAP: f32 = 8.0;
    pub const RAIL_W: f32 = 72.0;
    pub const INSPECTOR_W: f32 = 240.0;
    pub const TOP_BAR_H: f32 = 52.0;
    pub const STATUS_H: f32 = 28.0;
    pub const PALETTE_H: f32 = 132.0;
}

/// Rustbox spacing scale (showcase-style token set).
pub mod sp {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

/// Rustbox corner-radius scale.
pub mod radius {
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 18.0;
    pub const XL: f32 = 28.0;
}

pub fn col(r: u8, g: u8, b: u8) -> RColor {
    RColor::from_rgba(r, g, b, 255)
}

pub fn t(
    translations: &std::collections::HashMap<String, String>,
    key: &str,
    fallback: &str,
) -> String {
    translations
        .get(key)
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

pub fn tag_color(tag: LevelTag) -> RColor {
    match tag {
        LevelTag::Short => col(80, 150, 210),
        LevelTag::Puzzle => col(150, 110, 220),
        LevelTag::Precision => col(220, 110, 110),
        LevelTag::Chill => col(100, 180, 140),
        LevelTag::Music => col(220, 170, 100),
        LevelTag::Auto => col(120, 180, 200),
    }
}
