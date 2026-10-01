use repose_core::prelude::Color as RColor;

use crate::maker::level::LevelTag;

/// Rustbox UI tokens - dark creator workspace.
pub mod tok {
    use super::RColor;

    pub fn bg_deep() -> RColor {
        RColor::from_rgba(6, 8, 14, 255)
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
    pub fn scrim() -> RColor {
        RColor::from_rgba(0, 0, 0, 180)
    }

    pub fn accent() -> RColor {
        RColor::from_rgba(88, 166, 255, 255)
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

    pub fn bg_status() -> RColor {
        RColor::from_rgba(10, 12, 18, 245)
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

    pub const R_MD: f32 = 12.0;
    pub const R_PILL: f32 = 20.0;
}

/// Rustbox spacing scale (showcase-style token set).
pub mod sp {
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const XL: f32 = 24.0;
}

/// Rustbox corner-radius scale.
pub mod radius {
    pub const MD: f32 = 12.0;
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
