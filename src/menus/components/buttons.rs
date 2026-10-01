use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{AlignItems, Color as RColor, Dp, Modifier, Sp};
use repose_material::material3::{
    ButtonConfig, FilledTonalButton, FilledTonalIconButton, IconButtonColors, IconButtonConfig,
};
use repose_material::{Icon, Symbol};
use repose_ui::{Row, Text as RText, TextStyle, ViewExt};

use crate::menus::action::UiAction;
use crate::menus::style::tok;

pub fn spacer(h: f32) -> View {
    repose_ui::Column(Modifier::new().height(Dp(h)).width(Dp(1.0)))
}

pub fn push(actions: &Arc<Mutex<Vec<UiAction>>>, a: UiAction) {
    if let Ok(mut q) = actions.lock() {
        q.push(a);
    }
}

pub fn mk_button(label: &str, _bg: RColor, on_click: impl Fn() + 'static) -> View {
    let label = label.to_string();
    FilledTonalButton(
        Modifier::new()
            .width(Dp(260.0))
            .min_height(Dp(48.0))
            .margin(Dp(6.0))
            .clip_rounded(Dp(tok::R_MD)),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(Sp(18.0)),
    )
}

pub fn mk_button_sm(label: &str, on_click: impl Fn() + 'static) -> View {
    let label = label.to_string();
    FilledTonalButton(
        Modifier::new()
            .width(Dp(48.0))
            .height(Dp(40.0))
            .clip_rounded(Dp(tok::R_SM)),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(Sp(20.0)),
    )
}

pub fn mk_primary_button(label: View, bg: RColor, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .min_height(Dp(38.0))
            .padding(Dp(10.0))
            .background(bg)
            .clip_rounded(Dp(tok::R_SM))
            .flex_shrink(0.0),
        on_click,
        ButtonConfig::default(),
        move || label.clone(),
    )
}

pub fn mk_icon_button(icon: Symbol, enabled: bool, on_click: impl Fn() + 'static) -> View {
    FilledTonalIconButton(
        Icon(icon).size(Sp(19.0)),
        on_click,
        IconButtonConfig {
            enabled,
            container_size: Some(Dp(38.0)),
            colors: IconButtonColors {
                container_color: tok::bg_elevated(),
                content_color: tok::text(),
                disabled_container_color: tok::bg_panel_solid(),
                disabled_content_color: tok::text_mute(),
            },
            ..Default::default()
        },
    )
}

pub fn icon_label(symbol: Symbol, text: String) -> View {
    Row(Modifier::new().gap(Dp(6.0)).align_items(AlignItems::CENTER)).child((
        Icon(symbol).size(Sp(16.0)).color(tok::text()),
        RText(text).color(tok::text()),
    ))
}

pub fn icon_text(symbol: Symbol, text: String, size: f32, color: RColor) -> View {
    Row(Modifier::new().gap(Dp(5.0)).align_items(AlignItems::CENTER)).child((
        Icon(symbol).size(Sp(size)).color(color),
        RText(text).size(Sp(size)).color(color),
    ))
}

pub fn mk_pill_button(label: View, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .min_height(Dp(38.0))
            .padding(Dp(10.0))
            .clip_rounded(Dp(tok::R_PILL)),
        on_click,
        ButtonConfig::default(),
        move || label.clone(),
    )
}

pub fn mk_chip(label: View, selected: bool, accent: RColor, on_click: impl Fn() + 'static) -> View {
    let bg = if selected { accent } else { tok::bg_chip() };
    FilledTonalButton(
        Modifier::new()
            .min_height(Dp(32.0))
            .padding(Dp(8.0))
            .background(bg)
            .clip_rounded(Dp(tok::R_PILL))
            .flex_shrink(0.0),
        on_click,
        ButtonConfig::default(),
        move || label.clone(),
    )
}
