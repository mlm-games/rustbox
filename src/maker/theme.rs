//! Environment look for a level theme (sky clear color, ambient light, water
//! tint). Mirrors the Bevy original's `theme_env` (`Color::srgb` inputs),
//! converted to the linear RGB repame-view3d shades with.

use rustbox_format::Theme;

use super::level_view::srgb_to_linear;

/// Sky color, ambient floor and water tint for a level theme. Water alpha is
/// fixed by the original's translucent material (0.72).
pub struct ThemeEnv {
    pub sky: [f32; 3],
    pub ambient: f32,
    pub water: [f32; 3],
    pub water_alpha: f32,
}

pub fn theme_env(theme: Theme) -> ThemeEnv {
    let (sky, ambient, water) = match theme {
        Theme::Grass => ([0.53, 0.72, 0.92], 0.42, [0.18, 0.55, 0.9]),
        Theme::Desert => ([0.95, 0.78, 0.55], 0.46, [0.2, 0.7, 0.8]),
        Theme::Snow => ([0.82, 0.9, 0.96], 0.40, [0.35, 0.7, 0.95]),
        Theme::Cave => ([0.05, 0.05, 0.08], 0.16, [0.1, 0.3, 0.5]),
        Theme::Sky => ([0.3, 0.55, 1.0], 0.44, [0.2, 0.5, 0.9]),
    };
    ThemeEnv {
        sky: srgb_to_linear(sky),
        ambient,
        water: srgb_to_linear(water),
        water_alpha: 0.72,
    }
}