mod inspector;
mod pick;

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use repose_core::View;
use repose_core::prelude::{AlignItems, AlignSelf, Color, ImageFit, JustifyContent, Modifier};
use repose_material::material3::{
    ButtonConfig, FilledTonalButton, FilledTonalIconButton, IconButtonColors, IconButtonConfig,
};
use repose_material::{Icon, Symbol};
use repose_ui::{Column, Image, ImageExt, Row, Text as RText, TextStyle, ViewExt, ZStack};

use crate::app::SharedUi;
use crate::menus::action::UiAction;
use crate::menus::components::{Symbols, icon_text, push_ui};
use crate::menus::style::{t, tok};

pub use pick::part_picker;

const STRIP_SLOTS: usize = 9;

type PartRef = (u8, u8);

fn recents() -> MutexGuard<'static, Vec<PartRef>> {
    static R: OnceLock<Mutex<Vec<PartRef>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(default_recents()))
        .lock()
        .unwrap()
}

fn default_recents() -> Vec<PartRef> {
    // Grass, Stone, Water, Ice, Goal, Spawn, [Glimmer], [Prowler], Hazard
    vec![
        (0, 0),
        (0, 1),
        (0, 5),
        (0, 6),
        (0, 3),
        (0, 4),
        (1, 0),
        (1, 4),
        (0, 2),
    ]
}

pub(crate) fn prepend_recents(kind: u8, id: u8) {
    let mut ring = recents();
    if let Some(pos) = ring.iter().position(|&(k, i)| k == kind && i == id) {
        ring.remove(pos);
    }
    ring.insert(0, (kind, id));
    ring.truncate(STRIP_SLOTS);
}

fn selected_of(st: &SharedUi, kind: u8, id: u8) -> bool {
    match kind {
        1 => st.brush_tab == 1 && st.selected_entity == id,
        _ => st.brush_tab == 0 && st.selected_block == id,
    }
}

fn icon_of(st: &SharedUi, kind: u8, id: u8) -> Option<u64> {
    match kind {
        1 => st.entity_icon_handles.get(id as usize).copied(),
        _ => st.block_icon_handles.get(id as usize).copied(),
    }
}

pub fn ingame_hud(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    if !st.maker_mode_edit {
        return ZStack(Modifier::new().fill_max_size())
            .child(
                // Top-left: live run stats
                Column(
                    Modifier::new()
                        .fill_max_size()
                        .justify_content(JustifyContent::FLEX_START)
                        .align_items(AlignItems::FLEX_START)
                        .padding(14.0)
                        .gap(8.0),
                )
                .child(play_stats_bar(st)),
            )
            .child(
                // Bottom-left: BACK (return to the editor)
                Column(
                    Modifier::new()
                        .fill_max_size()
                        .justify_content(JustifyContent::FLEX_END)
                        .align_items(AlignItems::FLEX_START)
                        .padding(14.0)
                        .gap(8.0),
                )
                .child(clapperboard(st, actions)),
            )
            .child(status_toast(st));
    }

    ZStack(Modifier::new().fill_max_size())
        .child(
            // Top center: recs strip + held-part options row
            Column(
                Modifier::new()
                    .fill_max_width()
                    .align_items(AlignItems::CENTER)
                    .padding(10.0)
                    .gap(6.0),
            )
            .child((
                parts_strip(st, actions.clone()),
                held_options(st, actions.clone()),
            )),
        )
        .child(
            // Left edge, vertically centered: undo / redo
            Column(
                Modifier::new()
                    .fill_max_height()
                    .justify_content(JustifyContent::CENTER)
                    .align_items(AlignItems::FLEX_START)
                    .padding(10.0),
            )
            .child(left_rail(st, actions.clone())),
        )
        .child(
            // Right edge, vertically centered: Coursebot / settings / globe
            Column(
                Modifier::new()
                    .fill_max_size()
                    .justify_content(JustifyContent::CENTER)
                    .align_items(AlignItems::FLEX_END)
                    .padding(10.0),
            )
            .child(right_rail(actions.clone())),
        )
        .child(
            // Bottom-left: clapperboard
            Column(
                Modifier::new()
                    .fill_max_size()
                    .justify_content(JustifyContent::FLEX_END)
                    .align_items(AlignItems::FLEX_START)
                    .padding(14.0),
            )
            .child(clapperboard(st, actions.clone())),
        )
        .child(
            // Bottom-right: limits gauge
            Column(
                Modifier::new()
                    .fill_max_size()
                    .justify_content(JustifyContent::FLEX_END)
                    .align_items(AlignItems::FLEX_END)
                    .padding(14.0),
            )
            .child(limits_gauge(st)),
        )
        .child(selection_bubble(st, actions.clone()))
        .child(status_toast(st))
}

fn play_stats_bar(st: &SharedUi) -> View {
    let t = format_time(st.play_time_secs);
    let glimmer = if st.glimmers_total > 0 {
        format!("{}/{}", st.glimmers_collected, st.glimmers_total)
    } else {
        format!("{}", st.glimmers_collected)
    };
    let armor = st.player_armor;
    let keys: String = (1..=9)
        .filter(|&ch| st.player_keys.get(ch).copied().unwrap_or(0) > 0)
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>()
        .join("");
    let keys_label = if keys.is_empty() {
        "—".to_string()
    } else {
        keys
    };

    Column(
        Modifier::new()
            .gap(6.0)
            .padding(10.0)
            .background(tok::bg_elevated())
            .clip_rounded(tok::R_MD),
    )
    .child(stat_row(Symbols::TIMER, t))
    .child(stat_row(Symbols::SKULL, format!("{}", st.deaths)))
    .child(stat_row(Symbols::STAR, glimmer))
    .child(stat_row(Symbols::FAVORITE, format!("Armor {armor}")))
    .child(stat_row(Symbols::KEY, keys_label))
    .child(
        RText(st.level_name.clone())
            .size(12.0)
            .color(tok::text_dim()),
    )
}

/// `M:SS.s` style elapsed time.
fn format_time(secs: f32) -> String {
    let s = secs.max(0.0);
    let m = (s as u32) / 60;
    let r = s - (m as f32) * 60.0;
    format!("{m}:{:04.1}", r)
}

fn stat_row(symbol: Symbol, label: String) -> View {
    Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER)).child((
        Icon(symbol).size(18.0).color(tok::text()),
        RText(label).size(15.0).color(tok::text()),
    ))
}

fn status_toast(st: &SharedUi) -> View {
    if st.maker_status.is_empty() {
        return Row(Modifier::new().width(1.0).height(0.0));
    }
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::FLEX_START)
            .align_items(AlignItems::CENTER)
            .padding(18.0),
    )
    .child(
        Row(Modifier::new()
            .padding(12.0)
            .background(tok::bg_status())
            .clip_rounded(tok::R_PILL))
        .child(RText(st.maker_status.clone()).size(14.0).color(tok::text())),
    )
}

fn parts_strip(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let mut tiles: Vec<View> = Vec::new();
    let ring = recents();
    for (i, &(kind, id)) in ring.iter().take(STRIP_SLOTS).enumerate() {
        let selected = selected_of(st, kind, id);
        let icon = icon_of(st, kind, id);
        let a = actions.clone();
        tiles.push(part_tile(format!("{}", i + 1), icon, selected, move || {
            push_ui(&a, UiAction::MakerSetBrushTab(kind));
            push_ui(
                &a,
                if kind == 1 {
                    UiAction::MakerSelectEntity(id)
                } else {
                    UiAction::MakerSelectBlock(id)
                },
            );
        }));
    }

    let a_search = actions.clone();
    Row(Modifier::new()
        .padding(7.0)
        .gap(5.0)
        .align_items(AlignItems::CENTER)
        .background(tok::bg_elevated())
        .clip_rounded(tok::R_PILL))
    .children(tiles)
    .child(divider_dot())
    .child(icon_button(Symbols::SEARCH, true, move || {
        push_ui(&a_search, UiAction::OpenPartPicker)
    }))
}

fn part_tile(
    hotkey: String,
    icon: Option<u64>,
    selected: bool,
    on_click: impl Fn() + 'static,
) -> View {
    let ring = if selected {
        tok::accent()
    } else {
        tok::bg_panel_solid()
    };
    let pad = if selected { 3.0 } else { 2.0 };

    let mut inner_stack = ZStack(Modifier::new().fill_max_size());
    if let Some(handle) = icon {
        inner_stack = inner_stack
            .child(Image(Modifier::new().fill_max_size(), handle).image_fit(ImageFit::Cover));
    }
    inner_stack = inner_stack.child(RText(hotkey.clone()).size(14.0).color(tok::text_dim()));

    let inner = FilledTonalButton(
        Modifier::new()
            .fill_max_size()
            .background(tok::bg_elevated())
            .clip_rounded(tok::R_MD),
        on_click,
        ButtonConfig::default(),
        move || inner_stack.clone(),
    );
    ZStack(
        Modifier::new()
            .width(60.0)
            .height(60.0)
            .background(ring)
            .clip_rounded(tok::R_MD),
    )
    .child(inner)
}

/// Variant chips for the held part, shown directly beneath the strip.
fn held_options(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let mut chips: Vec<View> = Vec::new();
    if st.brush_tab == 0 {
        let shape_label = [
            "Full", "Half", "Top", "Slope", "DSlope", "Corner", "O.Corner", "V.Slope", "V.Slab",
            "Thin",
        ]
        .get(st.brush_shape as usize)
        .copied()
        .unwrap_or("Full");
        chips.push(option_chip(shape_label.to_string(), {
            let a = actions.clone();
            move || push_ui(&a, UiAction::MakerCycleShape)
        }));
        chips.push(option_chip(format!("{}°", st.brush_rot * 90), {
            let a = actions.clone();
            move || push_ui(&a, UiAction::MakerRotateBrushBlock)
        }));
        chips.push(option_chip(
            if st.waterlogged { "Wet" } else { "Dry" }.to_string(),
            {
                let a = actions.clone();
                move || push_ui(&a, UiAction::MakerToggleWaterlog)
            },
        ));
    } else if st.brush_tab == 1 {
        chips.push(option_chip("Rotate".to_string(), {
            let a = actions.clone();
            move || push_ui(&a, UiAction::MakerRotateBrush)
        }));
        if link_channelled(st.selected_entity) {
            chips.push(option_chip(format!("Ch {}", st.link_channel), {
                let a = actions.clone();
                move || push_ui(&a, UiAction::MakerCycleLinkChannel)
            }));
        }
    }
    if chips.is_empty() {
        return Row(Modifier::new().width(1.0).height(0.0));
    }
    Row(Modifier::new()
        .gap(6.0)
        .padding(4.0)
        .background(tok::bg_elevated())
        .clip_rounded(tok::R_PILL))
    .children(chips)
}

fn link_channelled(entity: u8) -> bool {
    matches!(entity, 5 | 6 | 8 | 12 | 13)
}

fn option_chip(label: String, on_click: impl Fn() + 'static) -> View {
    let label = label.clone();
    FilledTonalButton(
        Modifier::new()
            .min_height(36.0)
            .padding(12.0)
            .background(tok::bg_elevated())
            .clip_rounded(tok::R_PILL),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(13.0).color(tok::text()),
    )
}

fn left_rail(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_undo = actions.clone();
    let a_redo = actions.clone();
    rail_pill(vec![
        icon_button(Symbols::UNDO, st.can_undo, move || {
            push_ui(&a_undo, UiAction::MakerUndo)
        }),
        icon_button(Symbols::REDO, st.can_redo, move || {
            push_ui(&a_redo, UiAction::MakerRedo)
        }),
    ])
}

fn right_rail(actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_bot = actions.clone();
    let a_save = actions.clone();
    let a_set = actions.clone();
    let a_pub = actions.clone();
    rail_pill(vec![
        icon_button(Symbols::SMART_TOY, true, move || {
            push_ui(&a_bot, UiAction::MakerOpenLoadPanel)
        }),
        icon_button(Symbols::SAVE, true, move || {
            push_ui(&a_save, UiAction::MakerSave)
        }),
        icon_button(Symbols::SETTINGS, true, move || {
            push_ui(&a_set, UiAction::LevelInfoOpen)
        }),
        icon_button(Symbols::PUBLIC, true, move || {
            push_ui(&a_pub, UiAction::OnlineOpen)
        }),
    ])
}

fn rail_pill(children: Vec<View>) -> View {
    Column(
        Modifier::new()
            .padding(6.0)
            .gap(8.0)
            .background(tok::bg_elevated())
            .clip_rounded(tok::R_PILL),
    )
    .children(children)
}

fn clapperboard(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a = actions;
    let label = if st.maker_mode_edit { "PLAY" } else { "BACK" }.to_string();
    FilledTonalButton(
        Modifier::new()
            .min_height(56.0)
            .padding(16.0)
            .background(tok::danger())
            .clip_rounded(tok::R_PILL),
        move || push_ui(&a, UiAction::MakerToggleMode),
        ButtonConfig::default(),
        move || icon_text(Symbols::PLAY_ARROW, label.clone(), 18.0, Color::WHITE),
    )
}

fn limits_gauge(st: &SharedUi) -> View {
    let color = if st.limit_over {
        tok::danger()
    } else if st.limit_warning {
        tok::warn()
    } else {
        tok::ok()
    };
    const MAX_B: f32 = 20_000.0;
    const MAX_E: f32 = 1_000.0;
    let fb = (st.limit_blocks as f32 / MAX_B).clamp(0.0, 1.0);
    let fe = (st.limit_entities as f32 / MAX_E).clamp(0.0, 1.0);

    Column(
        Modifier::new()
            .padding(8.0)
            .gap(6.0)
            .background(tok::bg_elevated())
            .clip_rounded(tok::R_PILL),
    )
    .child(mini_bar(format!("B {}", st.limit_blocks), fb, color))
    .child(mini_bar(format!("E {}", st.limit_entities), fe, color))
    .child(
        RText(format!("T{} V{}", st.limit_tracks, st.limit_vertices))
            .size(11.0)
            .color(tok::text_dim()),
    )
}

fn mini_bar(label: String, frac: f32, color: Color) -> View {
    Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER)).child((
        RText(label).size(12.0).color(tok::text()),
        Column(
            Modifier::new()
                .width(72.0)
                .height(6.0)
                .background(tok::bg_panel_solid())
                .clip_rounded(3.0),
        )
        .child(Column(
            Modifier::new()
                .width((72.0 * frac).max(2.0))
                .height(6.0)
                .background(color)
                .clip_rounded(3.0),
        )),
    ))
}

/// Selection bubble: inspector panel, anchored right-center, only rendered
/// when a part is selected.
fn selection_bubble(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let has_selection = st.selected_entity_data.is_some() || st.active_track_data.is_some();
    if !has_selection {
        return Row(Modifier::new().width(1.0).height(0.0));
    }
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::FLEX_END)
            .padding(10.0),
    )
    .child(Row(Modifier::new().width(196.0)).child((
        inspector::inspector_panel(st, actions),
        Column(Modifier::new().width(56.0).height(1.0)),
    )))
}

fn divider_dot() -> View {
    Column(
        Modifier::new()
            .width(2.0)
            .height(28.0)
            .background(tok::bg_panel_solid())
            .clip_rounded(1.0)
            .align_self(AlignSelf::CENTER),
    )
}

fn icon_button(symbol: Symbol, enabled: bool, on_click: impl Fn() + 'static) -> View {
    FilledTonalIconButton(
        Icon(symbol).size(22.0).color(tok::text()),
        on_click,
        IconButtonConfig {
            enabled,
            container_size: Some(40.0),
            colors: IconButtonColors {
                container_color: tok::bg_elevated(),
                content_color: tok::text(),
                disabled_container_color: tok::bg_panel_solid(),
                disabled_content_color: tok::text_dim(),
            },
            ..Default::default()
        },
    )
}
