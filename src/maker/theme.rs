use bevy::prelude::*;

use rustbox_format::Theme;

/// Environment look for a level theme (sky clear color, ambient light, water
/// tint).
pub struct ThemeEnv {
    pub sky: Color,
    pub ambient: f32,
    pub water: Color,
}

pub fn theme_env(theme: Theme) -> ThemeEnv {
    match theme {
        Theme::Grass => ThemeEnv {
            sky: Color::srgb(0.53, 0.72, 0.92),
            ambient: 250.0,
            water: Color::srgb(0.18, 0.55, 0.9),
        },
        Theme::Desert => ThemeEnv {
            sky: Color::srgb(0.95, 0.78, 0.55),
            ambient: 280.0,
            water: Color::srgb(0.2, 0.7, 0.8),
        },
        Theme::Snow => ThemeEnv {
            sky: Color::srgb(0.82, 0.9, 0.96),
            ambient: 240.0,
            water: Color::srgb(0.35, 0.7, 0.95),
        },
        Theme::Cave => ThemeEnv {
            sky: Color::srgb(0.05, 0.05, 0.08),
            ambient: 90.0,
            water: Color::srgb(0.1, 0.3, 0.5),
        },
        Theme::Sky => ThemeEnv {
            sky: Color::srgb(0.3, 0.55, 1.0),
            ambient: 260.0,
            water: Color::srgb(0.2, 0.5, 0.9),
        },
    }
}
