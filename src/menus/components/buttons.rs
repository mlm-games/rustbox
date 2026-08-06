use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{AlignItems, Color as RColor, Modifier};
use repose_core::{ImageFit, PaddingValues};
use repose_material::material3::{
    ButtonConfig, FilledTonalButton, FilledTonalIconButton, IconButtonColors, IconButtonConfig,
};
use repose_material::{Icon, Symbol};
use repose_ui::{Image, ImageExt, Row, Text as RText, TextStyle, ViewExt, ZStack};

use crate::menus::action::UiAction;
use crate::menus::style::tok;

pub fn spacer(h: f32) -> View {
    repose_ui::Column(Modifier::new().height(h).width(1.0))
}

pub fn push(actions: &Arc<Mutex<Vec<UiAction>>>, a: UiAction) {
    if let Ok(mut q) = actions.lock() {
        q.push(a);
    }
}

pub fn push_ui(actions: &Arc<Mutex<Vec<UiAction>>>, a: UiAction) {
    push(actions, UiAction::SetPointerOverUi(true));
    push(actions, a);
}

pub fn mk_button(label: &str, _bg: RColor, on_click: impl Fn() + 'static) -> View {
    let label = label.to_string();
    FilledTonalButton(
        Modifier::new()
            .width(260.0)
            .min_height(48.0)
            .margin(6.0)
            .clip_rounded(tok::R_MD),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(18.0),
    )
}

pub fn mk_button_wide(label: &str, on_click: impl Fn() + 'static) -> View {
    let label = label.to_string();
    FilledTonalButton(
        Modifier::new()
            .fill_max_width()
            .min_height(48.0)
            .clip_rounded(tok::R_MD),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(17.0),
    )
}

pub fn mk_button_sm(label: &str, on_click: impl Fn() + 'static) -> View {
    let label = label.to_string();
    FilledTonalButton(
        Modifier::new()
            .width(48.0)
            .height(40.0)
            .clip_rounded(tok::R_SM),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(20.0),
    )
}

pub fn mk_pill_button(label: View, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .min_height(38.0)
            .padding(10.0)
            .clip_rounded(tok::R_PILL),
        on_click,
        ButtonConfig::default(),
        move || label.clone(),
    )
}

pub fn mk_primary_button(label: View, bg: RColor, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .min_height(38.0)
            .padding(10.0)
            .background(bg)
            .clip_rounded(tok::R_SM)
            .flex_shrink(0.0),
        on_click,
        ButtonConfig::default(),
        move || label.clone(),
    )
}

pub fn mk_icon_button(icon: Symbol, enabled: bool, on_click: impl Fn() + 'static) -> View {
    FilledTonalIconButton(
        Icon(icon).size(19.0),
        on_click,
        IconButtonConfig {
            enabled,
            container_size: Some(38.0),
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
    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
        Icon(symbol).size(16.0).color(tok::text()),
        RText(text).color(tok::text()),
    ))
}

pub fn icon_text(symbol: Symbol, text: String, size: f32, color: RColor) -> View {
    Row(Modifier::new().gap(5.0).align_items(AlignItems::CENTER)).child((
        Icon(symbol).size(size).color(color),
        RText(text).size(size).color(color),
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn mk_swatch(
    hotkey: String,
    color: RColor,
    icon: Option<u64>,
    selected: bool,
    on_click: impl Fn() + 'static,
) -> View {
    let bg = if selected { RColor::WHITE } else { color };
    FilledTonalButton(
        Modifier::new()
            .width(48.0)
            .height(48.0)
            .background(bg)
            .clip_rounded(tok::R_SM),
        on_click,
        ButtonConfig {
            content_padding: Some(PaddingValues {
                left: 3.0,
                right: 3.0,
                top: 3.0,
                bottom: 3.0,
            }),
            ..Default::default()
        },
        move || {
            let mut stack = ZStack(Modifier::new().fill_max_size());
            if let Some(handle) = icon {
                stack = stack.child(
                    Image(Modifier::new().fill_max_size().padding(2.0), handle)
                        .image_fit(ImageFit::Contain),
                );
            }
            stack.child(RText(hotkey.clone()).size(13.0).color(if selected {
                RColor::from_rgba(0, 0, 0, 255)
            } else {
                RColor::WHITE
            }))
        },
    )
}

pub fn mk_chip(label: View, selected: bool, accent: RColor, on_click: impl Fn() + 'static) -> View {
    let bg = if selected { accent } else { tok::bg_chip() };
    FilledTonalButton(
        Modifier::new()
            .min_height(32.0)
            .padding(8.0)
            .background(bg)
            .clip_rounded(tok::R_PILL),
        on_click,
        ButtonConfig::default(),
        move || label.clone(),
    )
}
