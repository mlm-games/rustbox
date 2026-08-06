use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{AlignItems, ImageFit, JustifyContent, Modifier, remember};
use repose_material::Icon;
use repose_material::material3::{ButtonConfig, FilledTonalButton, FilledTonalIconButton, IconButtonColors, IconButtonConfig};
use repose_ui::{Column, Image, ImageExt, Row, Text as RText, TextStyle, ViewExt, ZStack};

use crate::app::SharedUi;
use crate::menus::action::UiAction;
use crate::menus::components::{Symbols, push_ui};
use crate::menus::editor::prepend_recents;
use crate::menus::style::tok;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mm2Cat {
    Terrain,
    Items,
    Enemies,
    Gizmos,
}

struct PItem {
    kind: u8, // 0 block, 1 entity, 2 track
    id: u8,
    name: &'static str,
}

fn catalog(cat: Mm2Cat) -> Vec<PItem> {
    let it = |kind: u8, id: u8, name: &'static str| PItem { kind, id, name };
    match cat {
        Mm2Cat::Terrain => vec![
            it(0, 0, "Grass"),
            it(0, 1, "Stone"),
            it(0, 2, "Hazard"),
            it(0, 5, "Water"),
            it(0, 6, "Ice"),
            it(0, 7, "Spikes"),
            it(0, 4, "Spawn"),
            it(0, 3, "Goal"),
        ],
        Mm2Cat::Items => vec![
            it(1, 0, "Glimmer"),
            it(1, 12, "Key"),
            it(1, 14, "Heal"),
        ],
        Mm2Cat::Enemies => vec![it(1, 4, "Prowler"), it(1, 17, "Cannon")],
        Mm2Cat::Gizmos => vec![
            it(0, 8, "Conveyor"),
            it(0, 9, "Bounce"),
            it(0, 10, "Climb"),
            it(0, 11, "Thin Conv."),
            it(0, 12, "On/Off A"),
            it(0, 13, "On/Off B"),
            it(0, 14, "Hang Rail"),
            it(0, 15, "One Way"),
            it(0, 16, "Timed"),
            it(1, 5, "Trigger"),
            it(1, 6, "Relay"),
            it(1, 13, "Lock"),
            it(1, 18, "On/Off"),
            it(1, 8, "Teleport"),
            it(1, 1, "Launch"),
            it(1, 3, "Drift"),
            it(1, 9, "Fan"),
            it(1, 10, "Bumper"),
            it(1, 15, "Speed"),
            it(1, 2, "Seal"),
            it(1, 7, "Checkpoint"),
            it(1, 11, "Crate"),
            it(1, 16, "Crumble"),
            it(1, 19, "Toss Crate"),
            it(1, 20, "Sign"),
            it(2, 0, "Track"),
        ],
    }
}

pub fn part_picker(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tab: Rc<Cell<usize>> = remember(|| Cell::new(0));
    let cat = match tab.get() {
        1 => Mm2Cat::Items,
        2 => Mm2Cat::Enemies,
        3 => Mm2Cat::Gizmos,
        _ => Mm2Cat::Terrain,
    };

    let cats = ["Terrain", "Items", "Enemies", "Gizmos"];
    let mut tab_row: Vec<View> = Vec::new();
    for (i, label) in cats.iter().enumerate() {
        let t = tab.clone();
        let selected = i == tab.get();
        let name = (*label).to_string();
        tab_row.push(FilledTonalButton(
            Modifier::new()
                .min_height(44.0)
                .padding(18.0)
                .flex_grow(1.0)
                .background(if selected {
                    tok::bg_elevated()
                } else {
                    tok::bg_panel_solid()
                })
                .clip_rounded(tok::R_PILL),
            move || t.set(i),
            ButtonConfig::default(),
            move || {
                RText(name.clone())
                    .size(15.0)
                    .color(if selected { tok::text() } else { tok::text_dim() })
            },
        ));
    }

    let items = catalog(cat);
    let mut rows: Vec<View> = Vec::new();
    let mut row_kids: Vec<View> = Vec::new();
    for (n, item) in items.iter().enumerate() {
        let a = actions.clone();
        let kind = item.kind;
        let id = item.id;
        let name = item.name.to_string();
        let icon = icon_of(st, kind, id);
        row_kids.push(picker_tile(name, icon, move || {
            if kind == 2 {
                push_ui(&a, UiAction::MakerSetBrushTab(2));
            } else {
                push_ui(&a, UiAction::MakerSetBrushTab(kind));
                push_ui(
                    &a,
                    if kind == 1 {
                        UiAction::MakerSelectEntity(id)
                    } else {
                        UiAction::MakerSelectBlock(id)
                    },
                );
            }
            prepend_recents(kind, id);
            push_ui(&a, UiAction::CloseOverlay);
        }));
        if row_kids.len() == 6 || n + 1 == items.len() {
            rows.push(Row(Modifier::new().gap(10.0)).children(std::mem::take(&mut row_kids)));
        }
    }

    let panel = Column(
        Modifier::new()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
    )
    .child((
        Row(Modifier::new()
            .width(760.0)
            .gap(10.0)
            .padding(8.0)
            .background(tok::bg_elevated())
            .clip_rounded(tok::R_PILL))
        .children(tab_row),
        Column(Modifier::new().width(1.0).height(24.0)),
        Column(Modifier::new().gap(10.0).width(760.0).align_items(AlignItems::CENTER))
            .children(rows),
    ));

    let a_close = actions.clone();
    ZStack(Modifier::new().fill_max_size())
        .child(
            Column(
                Modifier::new()
                    .fill_max_size()
                    .background(tok::scrim())
                    .clickable()
                    .focusable(false)
                    .justify_content(JustifyContent::CENTER)
                    .align_items(AlignItems::CENTER),
            )
            .child(panel),
        )
        .child(Column(Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::FLEX_START)
            .align_items(AlignItems::FLEX_END)
            .padding(14.0))
        .child(FilledTonalIconButton(
            Icon(Symbols::CLOSE).size(24.0).color(tok::text()),
            move || push_ui(&a_close, UiAction::CloseOverlay),
            IconButtonConfig {
                enabled: true,
                container_size: Some(46.0),
                colors: IconButtonColors {
                    container_color: tok::bg_elevated(),
                    content_color: tok::text(),
                    disabled_container_color: tok::bg_panel_solid(),
                    disabled_content_color: tok::text_dim(),
                },
                ..Default::default()
            },
        )))
}

fn icon_of(st: &SharedUi, kind: u8, id: u8) -> Option<u64> {
    match kind {
        1 => st.entity_icon_handles.get(id as usize).copied(),
        2 => None,
        _ => st.block_icon_handles.get(id as usize).copied(),
    }
}

fn picker_tile(name: String, icon: Option<u64>, on_click: impl Fn() + 'static) -> View {
    let mut top = ZStack(Modifier::new()
        .width(64.0)
        .height(64.0)
        .background(tok::bg_panel_solid())
        .clip_rounded(tok::R_MD));
    if let Some(handle) = icon {
        top = top.child(
            Image(Modifier::new().fill_max_size().padding(4.0), handle)
                .image_fit(ImageFit::Contain),
        );
    } else {
        top = top.child(RText("T").size(26.0).color(tok::text()));
    }

    FilledTonalButton(
        Modifier::new()
            .width(72.0)
            .min_height(88.0)
            .padding(4.0)
            .background(tok::bg_elevated())
            .clip_rounded(tok::R_MD),
        on_click,
        ButtonConfig::default(),
        move || {
            Column(Modifier::new().gap(4.0).align_items(AlignItems::CENTER))
                .children(vec![top.clone(), RText(name.clone()).size(11.0).color(tok::text())])
        },
    )
}