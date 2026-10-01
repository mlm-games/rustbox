use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{
    AlignItems, Color as RColor, Dp, JustifyContent, Modifier, Sp, remember,
};
use repose_core::{ImeAction, KeyboardOptions, KeyboardType, TextFieldLineLimits};
use repose_material::Icon;
use repose_material::material3::{ButtonConfig, CardConfig, FilledTonalButton};
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::{
    BasicTextField, Column, FlowRow, FlowRowConfig, Row, Text as RText, TextFieldConfig,
    TextFieldState, TextStyle, ViewExt,
};

use crate::maker::catalog::{LevelSourceKind, LevelSummary, difficulty_label};
use crate::maker::level::LevelTag;
use crate::maker::thumbnail::ThumbPreview;
use crate::menus::action::UiAction;
use crate::menus::components::{
    Symbols, clickable_outlined_card, icon_label, mk_chip, mk_icon_button, modal_shell, push,
    spacer,
};
use crate::menus::style::{col, tag_color};
use crate::menus::{MenuState, OverlayMenu};

pub(crate) fn browse_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_close = actions.clone();
    let a_sort = actions.clone();
    let a_clear = actions.clone();
    let a_query = actions.clone();

    let levels = &st.browse_visible;

    let header = Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER))
    .child((
        RText("My Levels").size(Sp(28.0)).color(RColor::WHITE),
        Column(Modifier::new().fill_max_width()),
        mk_icon_button(Symbols::REMOVE, true, move || {
            push(&a_close, UiAction::CloseOverlay)
        }),
    ));

    let query_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let query_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !query_focus.get() && query_state.borrow().text != st.browse_query {
        query_state.borrow_mut().text = st.browse_query.clone();
    }
    let on_change = {
        let a = a_query.clone();
        Rc::new(move |v: String| push(&a, UiAction::BrowseSetQuery(v))) as Rc<dyn Fn(String)>
    };
    let field = BasicTextField(
        query_state.clone(),
        Modifier::new().flex_grow(1.0).height(Dp(34.0)),
        "Search name, author, description",
        TextFieldConfig {
            line_limits: TextFieldLineLimits::SingleLine,
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Filter,
                ime_action: ImeAction::Search,
                ..KeyboardOptions::DEFAULT
            },
            on_change: Some(on_change),
            focus_tracker: Some(query_focus.clone()),
            ..Default::default()
        },
    );

    let mut search_children: Vec<View> = vec![
        Icon(Symbols::SEARCH)
            .size(Sp(18.0))
            .color(col(150, 150, 170)),
        field,
    ];
    if !st.browse_query.is_empty() {
        let qs = query_state.clone();
        let a_clear2 = a_clear.clone();
        search_children.push(mk_icon_button(Symbols::REMOVE, true, move || {
            qs.borrow_mut().text.clear();
            push(&a_clear2, UiAction::BrowseClearQuery)
        }));
    }
    let search_row = Row(Modifier::new()
        .fill_max_width()
        .gap(Dp(8.0))
        .align_items(AlignItems::CENTER)
        .padding(Dp(10.0))
        .background(col(45, 45, 60))
        .clip_rounded(Dp(18.0)))
    .child(search_children);

    // Tell the Bevy-side browser nav to stand down while a text field owns focus.
    if matches!(st.overlay, OverlayMenu::Browse) {
        push(&actions, UiAction::SetKeyboardCaptured(query_focus.get()));
    }

    let hint =
        RText("Try: name:air  author:mlm  tag:puzzle  #precision  verified:1  diff:hard  has:gate")
            .size(Sp(11.0))
            .color(col(130, 130, 150));

    // Verified + difficulty chips (filters, not level tags).
    let verified_label = if st.browse_verified_only {
        icon_label(Symbols::CHECK, "Verified".into())
    } else {
        RText("Verified").size(Sp(12.0)).color(col(150, 150, 170))
    };
    let a_ver = actions.clone();
    let mut filter_chips: Vec<View> = vec![mk_chip(
        verified_label,
        st.browse_verified_only,
        col(90, 200, 120),
        move || push(&a_ver, UiAction::BrowseToggleVerified),
    )];
    for i in 0..4u8 {
        let a = actions.clone();
        let selected = st.browse_difficulty == Some(i);
        let label = RText(difficulty_label(i))
            .size(Sp(12.0))
            .color(if selected {
                RColor::WHITE
            } else {
                col(150, 150, 170)
            });
        let diff_color = match i {
            0 => col(100, 180, 140),
            1 => col(150, 170, 90),
            2 => col(220, 150, 90),
            _ => col(220, 90, 90),
        };
        filter_chips.push(mk_chip(label, selected, diff_color, move || {
            push(
                &a,
                UiAction::BrowseSetDifficulty(if selected { None } else { Some(i) }),
            )
        }));
    }
    let filter_row = FlowRow(
        Modifier::new().fill_max_width().gap(Dp(6.0)),
        FlowRowConfig::default(),
    )
    .child(filter_chips);

    // Tag include chips.
    let mut chip_views: Vec<View> = Vec::new();
    for tag in LevelTag::ALL {
        let a = actions.clone();
        let included = st.browse_include_tags.contains(&tag);
        let label = RText(tag.label()).size(Sp(12.0)).color(if included {
            RColor::WHITE
        } else {
            col(150, 150, 170)
        });
        chip_views.push(mk_chip(label, included, tag_color(tag), move || {
            push(&a, UiAction::BrowseToggleTag(tag))
        }));
    }
    let tag_row = FlowRow(
        Modifier::new().fill_max_width().gap(Dp(6.0)),
        FlowRowConfig::default(),
    )
    .child(chip_views);

    let sort_label = match st.browse_sort % 6 {
        0 => "Sort: Recent",
        1 => "Sort: Name",
        2 => "Sort: Shortest",
        3 => "Sort: Longest",
        4 => "Sort: Fastest clear",
        _ => "Sort: Hardest",
    };
    let sort_text = RText(sort_label).size(Sp(13.0)).color(col(190, 190, 205));
    let sort_button = FilledTonalButton(
        Modifier::new()
            .height(Dp(30.0))
            .padding(Dp(12.0))
            .background(col(45, 45, 60))
            .clip_rounded(Dp(15.0)),
        move || push(&a_sort, UiAction::BrowseCycleSort),
        ButtonConfig::default(),
        move || sort_text.clone(),
    );
    let count_text = RText(format!("{} levels", levels.len()))
        .size(Sp(12.0))
        .color(col(150, 150, 170));
    let sort_row = Row(Modifier::new()
        .gap(Dp(12.0))
        .align_items(AlignItems::CENTER))
    .child((sort_button, count_text));

    let mut grid_children: Vec<View> = Vec::new();
    if st.browse_levels.is_empty() {
        grid_children.push(
            RText("Import a share code or save to your collection from Share.")
                .size(Sp(14.0))
                .color(col(180, 180, 190)),
        );
    } else if levels.is_empty() {
        grid_children.push(
            RText("No matches. Try author:, tag:puzzle, has:gate, verified:1.")
                .size(Sp(14.0))
                .color(col(180, 180, 190)),
        );
    } else {
        for s in levels {
            grid_children.push(browse_card(s, st, &actions));
        }
    }

    let scroll_state = remember_scroll_state("browse_list");
    let grid_view = repose_ui::Grid(
        3,
        Modifier::new().fill_max_width(),
        grid_children,
        Dp(10.0),
        Dp(10.0),
    );
    let scroll_list = ScrollArea(
        Modifier::new().fill_max_width().weight(1.0),
        scroll_state,
        grid_view,
    );

    // Detail panel: identity + actions, rendered when a card is selected.
    let detail: View = match st
        .browse_visible
        .iter()
        .find(|s| st.browse_selected.as_deref() == Some(s.key.as_str()))
    {
        Some(sd) => local_detail_panel(sd, st, &actions),
        None => BrowseSelectPanel(vec![
            RText("Select a level")
                .size(Sp(14.0))
                .color(col(150, 150, 170)),
        ]),
    };

    let inner = Column(
        Modifier::new()
            .fill_max_width()
            .width(Dp(980.0))
            .max_height(Dp(760.0))
            .padding(Dp(24.0))
            .background(col(20, 20, 28))
            .clip_rounded(Dp(12.0))
            .align_items(AlignItems::CENTER),
    )
    .child(header)
    .child(spacer(12.0))
    .child(search_row)
    .child(spacer(6.0))
    .child(hint)
    .child(spacer(10.0))
    .child(filter_row)
    .child(spacer(8.0))
    .child(tag_row)
    .child(spacer(10.0))
    .child(sort_row)
    .child(spacer(12.0))
    .child(
        Row(Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER))
        .child((
            Column(Modifier::new().fill_max_width().weight(1.0)).child(scroll_list),
            detail,
        )),
    );

    modal_shell(inner)
}

fn diff_strip_color(difficulty: u8) -> RColor {
    match difficulty {
        0 => col(100, 180, 140),
        1 => col(150, 170, 90),
        2 => col(220, 150, 90),
        _ => col(220, 90, 90),
    }
}

fn browse_card(s: &LevelSummary, st: &MenuState, actions: &Arc<Mutex<Vec<UiAction>>>) -> View {
    let b_sel = actions.clone();
    let k = s.key.clone();
    let selected = st.browse_selected.as_deref() == Some(s.key.as_str());

    let mut name_children: Vec<View> =
        vec![RText(s.name.clone()).size(Sp(16.0)).color(RColor::WHITE)];
    if s.verified {
        name_children.push(
            Icon(Symbols::CHECK)
                .size(Sp(15.0))
                .color(col(220, 210, 120)),
        );
    }
    if s.source == LevelSourceKind::Collection {
        name_children.push(
            Icon(Symbols::FOLDER_OPEN)
                .size(Sp(14.0))
                .color(col(150, 150, 170)),
        );
    }

    let card_config = selected_card_config(selected);

    clickable_outlined_card(
        move || push(&b_sel, UiAction::BrowseSelect(k.clone())),
        Modifier::new().fill_max_width(),
        card_config,
        move || {
            Column(
                Modifier::new()
                    .gap(Dp(6.0))
                    .align_items(AlignItems::FLEX_START),
            )
            .child((
                // Thumbnail as the card's face (identity only).
                card_preview_box(Some(thumb_grid_view(&s.preview)), 96.0),
                Row(Modifier::new().gap(Dp(6.0)).align_items(AlignItems::CENTER))
                    .child(name_children),
                Row(Modifier::new().gap(Dp(8.0)).align_items(AlignItems::CENTER)).child((
                    Column(
                        Modifier::new()
                            .width(Dp(46.0))
                            .height(Dp(18.0))
                            .background(diff_strip_color(s.difficulty))
                            .clip_rounded(Dp(9.0))
                            .align_items(AlignItems::CENTER),
                    )
                    .child(
                        RText(difficulty_label(s.difficulty))
                            .size(Sp(11.0))
                            .color(RColor::from_rgba(0, 0, 0, 230)),
                    ),
                    RText(if s.author.is_empty() {
                        "Unknown".to_string()
                    } else {
                        s.author.clone()
                    })
                    .size(Sp(12.0))
                    .color(col(150, 150, 170)),
                )),
            ))
        },
    )
}

fn local_detail_panel(
    s: &LevelSummary,
    st: &MenuState,
    actions: &Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let a_play = actions.clone();
    let a_edit = actions.clone();
    let a_del = actions.clone();
    let k_play = s.key.clone();
    let k_edit = s.key.clone();
    let k_del = s.key.clone();

    let mut name_children: Vec<View> =
        vec![RText(s.name.clone()).size(Sp(20.0)).color(RColor::WHITE)];
    if s.verified {
        name_children.push(
            Icon(Symbols::CHECK)
                .size(Sp(16.0))
                .color(col(220, 210, 120)),
        );
    }

    let mut tag_pills: Vec<View> = Vec::new();
    for tag in &s.tags {
        tag_pills.push(
            Column(
                Modifier::new()
                    .padding(Dp(6.0))
                    .background(tag_color(*tag))
                    .clip_rounded(Dp(8.0)),
            )
            .child(RText(tag.label()).size(Sp(11.0)).color(col(170, 170, 190))),
        );
    }

    let mut stats = format!("{} blocks · {} ents", s.block_count, s.entity_count);
    if s.track_count > 0 {
        stats.push_str(&format!(" · {} tracks", s.track_count));
    }
    tag_pills.push(RText(stats).size(Sp(11.0)).color(col(140, 140, 160)));

    let confirming_delete = st.browse_confirm_delete.as_deref() == Some(s.key.as_str());

    let action_row: View = if confirming_delete {
        let a_confirm = actions.clone();
        let a_cancel = actions.clone();
        let k_confirm = s.key.clone();

        Column(Modifier::new().gap(Dp(6.0))).child((
            RText("Delete this level?")
                .size(Sp(12.0))
                .color(col(232, 120, 120)),
            Row(Modifier::new().gap(Dp(6.0)).align_items(AlignItems::CENTER)).child((
                FilledTonalButton(
                    Modifier::new()
                        .height(Dp(34.0))
                        .background(col(160, 60, 60))
                        .clip_rounded(Dp(8.0)),
                    move || push(&a_confirm, UiAction::BrowseConfirmDelete(k_confirm.clone())),
                    ButtonConfig::default(),
                    move || RText("Confirm Delete").size(Sp(13.0)).color(RColor::WHITE),
                ),
                FilledTonalButton(
                    Modifier::new().height(Dp(34.0)),
                    move || push(&a_cancel, UiAction::BrowseCancelDelete),
                    ButtonConfig::default(),
                    move || RText("Cancel").size(Sp(13.0)).color(RColor::WHITE),
                ),
            )),
        ))
    } else {
        Row(Modifier::new().gap(Dp(6.0)).align_items(AlignItems::CENTER)).child((
            FilledTonalButton(
                Modifier::new().height(Dp(34.0)),
                move || push(&a_play, UiAction::BrowsePlay(k_play.clone())),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(Dp(6.0)).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::PLAY_ARROW)
                            .size(Sp(18.0))
                            .color(RColor::WHITE),
                        RText("Play").size(Sp(14.0)).color(RColor::WHITE),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(Dp(34.0)),
                move || push(&a_edit, UiAction::BrowseEdit(k_edit.clone())),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(Dp(6.0)).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::EDIT).size(Sp(16.0)).color(RColor::WHITE),
                        RText("Edit").size(Sp(14.0)).color(RColor::WHITE),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(Dp(34.0)),
                move || push(&a_del, UiAction::BrowseDelete(k_del.clone())),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(Dp(6.0)).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::DELETE)
                            .size(Sp(16.0))
                            .color(col(232, 120, 120)),
                        RText("Delete").size(Sp(14.0)).color(col(232, 120, 120)),
                    ))
                },
            ),
        ))
    };

    Column(
        Modifier::new()
            .width(Dp(280.0))
            .fill_max_height()
            .background(col(24, 24, 34))
            .clip_rounded(Dp(12.0))
            .padding(Dp(14.0))
            .gap(Dp(8.0)),
    )
    .child((
        // Large preview
        card_preview_box(Some(thumb_grid_view(&s.preview)), 120.0),
        Row(Modifier::new().gap(Dp(6.0)).align_items(AlignItems::CENTER)).child(name_children),
        RText(if s.author.is_empty() {
            "Unknown".to_string()
        } else {
            s.author.clone()
        })
        .size(Sp(12.0))
        .color(col(150, 150, 170)),
        Row(Modifier::new().gap(Dp(6.0)).align_items(AlignItems::CENTER)).child(tag_pills),
        action_row,
    ))
}

/// Right-side detail panel shown before any card is selected.
#[allow(non_snake_case)]
fn BrowseSelectPanel(views: Vec<View>) -> View {
    Column(
        Modifier::new()
            .width(Dp(280.0))
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
    )
    .with_children(views)
}

pub fn preview_thumb(p: &ThumbPreview, cell: f32) -> View {
    let mut rows: Vec<View> = Vec::with_capacity(p.rows);
    for r in 0..p.rows {
        let mut cells: Vec<View> = Vec::with_capacity(p.cols);
        for cidx in 0..p.cols {
            let px = p.cells[r * p.cols + cidx];
            cells.push(Column(
                Modifier::new()
                    .width(Dp(cell))
                    .height(Dp(cell))
                    .background(RColor::from_rgba(px[0], px[1], px[2], px[3])),
            ));
        }
        rows.push(Row(Modifier::new()).child(cells));
    }

    Column(
        Modifier::new()
            .clip_rounded(Dp(8.0))
            .background(col(30, 30, 40)),
    )
    .child(rows)
}

fn thumb_grid_view(p: &ThumbPreview) -> View {
    preview_thumb(p, 4.0)
}

/// Shared card chrome for the Browse / Online identity-only grids: a thin
/// light border, a thicker amber border + highlight once selected.
fn selected_card_config(selected: bool) -> CardConfig {
    CardConfig {
        border: Some((
            if selected { Dp(2.0) } else { Dp(1.0) },
            if selected {
                col(255, 217, 59)
            } else {
                RColor::from_rgba(255, 255, 255, 40)
            },
        )),
        shape_radius: Dp(12.0),
        ..Default::default()
    }
}

/// Shared card face: a fixed-height box that centers either a generated
/// thumbnail or a placeholder (used by both browse and online cards/detail).
fn card_preview_box(content: Option<View>, height: f32) -> View {
    Column(
        Modifier::new()
            .fill_max_width()
            .height(Dp(height))
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .background(col(30, 30, 42))
            .clip_rounded(Dp(8.0)),
    )
    .child(content.unwrap_or_else(|| RText("…").size(Sp(14.0)).color(col(120, 120, 145))))
}
