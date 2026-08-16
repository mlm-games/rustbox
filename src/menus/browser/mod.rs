use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use repose_core::View;
use repose_core::prelude::{
    AlignItems, AnimationSpec, Color as RColor, Easing, JustifyContent, Modifier, remember,
    remember_mutable,
};
use repose_core::{ImeAction, KeyboardOptions, KeyboardType, TextFieldLineLimits};
use repose_material::Icon;
use repose_material::material3::{
    ButtonConfig, CardConfig, ChipConfig, FilledTonalButton, InputChip,
};
use repose_ui::anim_ext::{AnimatedVisibility, AnimatedVisibilityConfig};
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::{
    BasicTextField, Column, FlowRow, Row, Text as RText, TextFieldConfig, TextFieldState,
    TextStyle, ViewExt,
};

use crate::app::{OverlayMenu, SharedUi};
use crate::maker::catalog::{LevelSourceKind, LevelSummary, difficulty_label};
use crate::maker::level::LevelTag;
use crate::maker::thumbnail::ThumbPreview;
use crate::menus::action::UiAction;
use crate::menus::components::{
    Symbols, clickable_outlined_card, icon_label, icon_text, mk_chip, mk_icon_button,
    mk_pill_button, mk_primary_button, modal_shell, push, spacer,
};
use crate::menus::style::{col, tag_color};
use rustbox_format::api::LevelMeta;

pub(crate) fn browse_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_close = actions.clone();
    let a_sort = actions.clone();
    let a_clear = actions.clone();
    let a_query = actions.clone();

    let levels = &st.browse_visible;

    let header = Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER))
    .child((
        RText("My Levels").size(28.0).color(RColor::WHITE),
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
        Modifier::new().flex_grow(1.0).height(34.0),
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
        Icon(Symbols::SEARCH).size(18.0).color(col(150, 150, 170)),
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
        .gap(8.0)
        .align_items(AlignItems::CENTER)
        .padding(10.0)
        .background(col(45, 45, 60))
        .clip_rounded(18.0))
    .child(search_children);

    // Tell the Bevy-side browser nav to stand down while a text field owns focus.
    if matches!(st.overlay, OverlayMenu::Browse) {
        push(&actions, UiAction::SetKeyboardCaptured(query_focus.get()));
    }

    let hint =
        RText("Try: name:air  author:mlm  tag:puzzle  #precision  verified:1  diff:hard  has:gate")
            .size(11.0)
            .color(col(130, 130, 150));

    // Verified + difficulty chips (filters, not level tags).
    let verified_label = if st.browse_verified_only {
        icon_label(Symbols::CHECK, "Verified".into())
    } else {
        RText("Verified").size(12.0).color(col(150, 150, 170))
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
        let label = RText(difficulty_label(i)).size(12.0).color(if selected {
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
    let filter_row = FlowRow(Modifier::new().fill_max_width().gap(6.0)).child(filter_chips);

    // Tag include chips.
    let mut chip_views: Vec<View> = Vec::new();
    for tag in LevelTag::ALL {
        let a = actions.clone();
        let included = st.browse_include_tags.contains(&tag);
        let label = RText(tag.label()).size(12.0).color(if included {
            RColor::WHITE
        } else {
            col(150, 150, 170)
        });
        chip_views.push(mk_chip(label, included, tag_color(tag), move || {
            push(&a, UiAction::BrowseToggleTag(tag))
        }));
    }
    let tag_row = FlowRow(Modifier::new().fill_max_width().gap(6.0)).child(chip_views);

    let sort_label = match st.browse_sort % 6 {
        0 => "Sort: Recent",
        1 => "Sort: Name",
        2 => "Sort: Shortest",
        3 => "Sort: Longest",
        4 => "Sort: Fastest clear",
        _ => "Sort: Hardest",
    };
    let sort_text = RText(sort_label).size(13.0).color(col(190, 190, 205));
    let sort_button = FilledTonalButton(
        Modifier::new()
            .height(30.0)
            .padding(12.0)
            .background(col(45, 45, 60))
            .clip_rounded(15.0),
        move || push(&a_sort, UiAction::BrowseCycleSort),
        ButtonConfig::default(),
        move || sort_text.clone(),
    );
    let count_text = RText(format!("{} levels", levels.len()))
        .size(12.0)
        .color(col(150, 150, 170));
    let sort_row = Row(Modifier::new().gap(12.0).align_items(AlignItems::CENTER))
        .child((sort_button, count_text));

    let mut grid_children: Vec<View> = Vec::new();
    if st.browse_levels.is_empty() {
        grid_children.push(
            RText("Import a share code or save to your collection from Share.")
                .size(14.0)
                .color(col(180, 180, 190)),
        );
    } else if levels.is_empty() {
        grid_children.push(
            RText("No matches. Try author:, tag:puzzle, has:gate, verified:1.")
                .size(14.0)
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
        10.0,
        10.0,
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
            RText("Select a level").size(14.0).color(col(150, 150, 170)),
        ]),
    };

    let inner = Column(
        Modifier::new()
            .fill_max_width()
            .width(980.0)
            .max_height(760.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
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

fn browse_card(s: &LevelSummary, st: &SharedUi, actions: &Arc<Mutex<Vec<UiAction>>>) -> View {
    let b_sel = actions.clone();
    let k = s.key.clone();
    let selected = st.browse_selected.as_deref() == Some(s.key.as_str());

    let mut name_children: Vec<View> = vec![RText(s.name.clone()).size(16.0).color(RColor::WHITE)];
    if s.verified {
        name_children.push(Icon(Symbols::CHECK).size(15.0).color(col(220, 210, 120)));
    }
    if s.source == LevelSourceKind::Collection {
        name_children.push(
            Icon(Symbols::FOLDER_OPEN)
                .size(14.0)
                .color(col(150, 150, 170)),
        );
    }

    let card_config = selected_card_config(selected);

    clickable_outlined_card(
        move || push(&b_sel, UiAction::BrowseSelect(k.clone())),
        Modifier::new().fill_max_width(),
        card_config,
        move || {
            Column(Modifier::new().gap(6.0).align_items(AlignItems::FLEX_START)).child((
                // Thumbnail as the card's face (identity only).
                card_preview_box(Some(thumb_grid_view(&s.preview)), 96.0),
                Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child(name_children),
                Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER)).child((
                    Column(
                        Modifier::new()
                            .width(46.0)
                            .height(18.0)
                            .background(diff_strip_color(s.difficulty))
                            .clip_rounded(9.0)
                            .align_items(AlignItems::CENTER),
                    )
                    .child(
                        RText(difficulty_label(s.difficulty))
                            .size(11.0)
                            .color(RColor::from_rgba(0, 0, 0, 230)),
                    ),
                    RText(if s.author.is_empty() {
                        "Unknown".to_string()
                    } else {
                        s.author.clone()
                    })
                    .size(12.0)
                    .color(col(150, 150, 170)),
                )),
            ))
        },
    )
}

fn local_detail_panel(
    s: &LevelSummary,
    st: &SharedUi,
    actions: &Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let a_play = actions.clone();
    let a_edit = actions.clone();
    let a_del = actions.clone();
    let a_pub = actions.clone();
    let k_play = s.key.clone();
    let k_edit = s.key.clone();
    let k_del = s.key.clone();
    let k_pub = s.key.clone();

    let mut name_children: Vec<View> = vec![RText(s.name.clone()).size(20.0).color(RColor::WHITE)];
    if s.verified {
        name_children.push(Icon(Symbols::CHECK).size(16.0).color(col(220, 210, 120)));
    }

    let mut tag_pills: Vec<View> = Vec::new();
    for tag in &s.tags {
        tag_pills.push(
            Column(
                Modifier::new()
                    .padding(6.0)
                    .background(tag_color(*tag))
                    .clip_rounded(8.0),
            )
            .child(RText(tag.label()).size(11.0).color(col(170, 170, 190))),
        );
    }

    let mut stats = format!("{} blocks · {} ents", s.block_count, s.entity_count);
    if s.track_count > 0 {
        stats.push_str(&format!(" · {} tracks", s.track_count));
    }
    tag_pills.push(RText(stats).size(11.0).color(col(140, 140, 160)));

    let confirming_delete = st.browse_confirm_delete.as_deref() == Some(s.key.as_str());

    let action_row: View = if confirming_delete {
        let a_confirm = actions.clone();
        let a_cancel = actions.clone();
        let k_confirm = s.key.clone();

        Column(Modifier::new().gap(6.0)).child((
            RText("Delete this level?")
                .size(12.0)
                .color(col(232, 120, 120)),
            Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                FilledTonalButton(
                    Modifier::new()
                        .height(34.0)
                        .background(col(160, 60, 60))
                        .clip_rounded(8.0),
                    move || push(&a_confirm, UiAction::BrowseConfirmDelete(k_confirm.clone())),
                    ButtonConfig::default(),
                    move || RText("Confirm Delete").size(13.0).color(RColor::WHITE),
                ),
                FilledTonalButton(
                    Modifier::new().height(34.0),
                    move || push(&a_cancel, UiAction::BrowseCancelDelete),
                    ButtonConfig::default(),
                    move || RText("Cancel").size(13.0).color(RColor::WHITE),
                ),
            )),
        ))
    } else {
        Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_play, UiAction::BrowsePlay(k_play.clone())),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::PLAY_ARROW).size(18.0).color(RColor::WHITE),
                        RText("Play").size(14.0).color(RColor::WHITE),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_edit, UiAction::BrowseEdit(k_edit.clone())),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::EDIT).size(16.0).color(RColor::WHITE),
                        RText("Edit").size(14.0).color(RColor::WHITE),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_del, UiAction::BrowseDelete(k_del.clone())),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::DELETE).size(16.0).color(col(232, 120, 120)),
                        RText("Delete").size(14.0).color(col(232, 120, 120)),
                    ))
                },
            ),
        ))
    };

    Column(
        Modifier::new()
            .width(280.0)
            .fill_max_height()
            .background(col(24, 24, 34))
            .clip_rounded(12.0)
            .padding(14.0)
            .gap(8.0),
    )
    .child((
        // Large preview
        card_preview_box(Some(thumb_grid_view(&s.preview)), 120.0),
        Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child(name_children),
        RText(if s.author.is_empty() {
            "Unknown".to_string()
        } else {
            s.author.clone()
        })
        .size(12.0)
        .color(col(150, 150, 170)),
        Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child(tag_pills),
        FilledTonalButton(
            Modifier::new()
                .height(36.0)
                .fill_max_width()
                .background(if s.verified {
                    col(120, 90, 200)
                } else {
                    col(70, 60, 90)
                })
                .clip_rounded(8.0),
            move || push(&a_pub, UiAction::BrowsePublish(k_pub.clone())),
            ButtonConfig::default(),
            move || {
                Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                    Icon(Symbols::CLOUD_UPLOAD).size(16.0).color(RColor::WHITE),
                    RText(if s.verified {
                        "Publish".to_string()
                    } else {
                        "Beat to publish".to_string()
                    })
                    .size(14.0)
                    .color(RColor::WHITE),
                ))
            },
        ),
        action_row,
    ))
}

/// Right-side detail panel shown before any card is selected.
#[allow(non_snake_case)]
fn BrowseSelectPanel(views: Vec<View>) -> View {
    Column(
        Modifier::new()
            .width(280.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
    )
    .with_children(views)
}

pub(crate) fn online_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_close = actions.clone();
    let a_refresh = actions.clone();
    let a_clear = actions.clone();
    let a_submit_search = actions.clone();
    let a_submit_token = actions.clone();
    let a_search = actions.clone();
    let a_set_token = actions.clone();
    let a_upload = actions.clone();
    let a_sort = actions.clone();

    let levels = &st.online_levels;

    let header = Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER))
    .child((
        RText("Online Levels").size(28.0).color(RColor::WHITE),
        Column(Modifier::new().fill_max_width()),
        mk_icon_button(Symbols::REFRESH, true, move || {
            push(&a_refresh, UiAction::OnlineRefresh)
        }),
        mk_icon_button(Symbols::REMOVE, true, move || {
            push(&a_close, UiAction::CloseOverlay)
        }),
    ));

    let query_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let query_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !query_focus.get() && query_state.borrow().text != st.online_query {
        query_state.borrow_mut().text = st.online_query.clone();
    }
    let on_change = {
        let a = actions.clone();
        Rc::new(move |v: String| push(&a, UiAction::OnlineSetQuery(v))) as Rc<dyn Fn(String)>
    };
    let on_submit = {
        let a = a_submit_search.clone();
        Rc::new(move |v: String| {
            push(&a, UiAction::OnlineSetQuery(v));
            push(&a, UiAction::OnlineSearch);
        }) as Rc<dyn Fn(String)>
    };
    let search_field = BasicTextField(
        query_state.clone(),
        Modifier::new().flex_grow(1.0).height(34.0),
        "Search name or author",
        TextFieldConfig {
            line_limits: TextFieldLineLimits::SingleLine,
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Filter,
                ime_action: ImeAction::Search,
                ..KeyboardOptions::DEFAULT
            },
            on_change: Some(on_change),
            on_submit: Some(on_submit),
            focus_tracker: Some(query_focus.clone()),
            ..Default::default()
        },
    );

    let mut search_children: Vec<View> = vec![
        Icon(Symbols::SEARCH).size(18.0).color(col(150, 150, 170)),
        search_field,
    ];
    if !st.online_query.is_empty() {
        let qs = query_state.clone();
        let a_clear2 = a_clear.clone();
        search_children.push(mk_icon_button(Symbols::REMOVE, true, move || {
            qs.borrow_mut().text.clear();
            push(&a_clear2, UiAction::OnlineClearQuery)
        }));
    }
    let search_row = Row(Modifier::new()
        .fill_max_width()
        .gap(8.0)
        .align_items(AlignItems::CENTER))
    .child((
        Row(Modifier::new()
            .fill_max_width()
            .gap(8.0)
            .align_items(AlignItems::CENTER)
            .padding(10.0)
            .background(col(45, 45, 60))
            .clip_rounded(18.0))
        .child(search_children),
        mk_pill_button(icon_label(Symbols::SEARCH, "Search".into()), move || {
            let q = query_state.borrow().text.clone();
            push(&a_search, UiAction::OnlineSetQuery(q));
            push(&a_search, UiAction::OnlineSearch);
        }),
    ));

    // Anonymous creator identity: the recovery key IS the account.
    let creator_short = if st.creator_recovery_key.is_empty() {
        "…".to_string()
    } else {
        crate::maker::creator::short_maker_id(&st.creator_recovery_key)
    };
    let reveal_state: Rc<Cell<bool>> = remember(|| Cell::new(false));
    let show_key = reveal_state.get();
    let a_copy_key = actions.clone();

    let identity_box = Row(Modifier::new()
        .fill_max_width()
        .gap(8.0)
        .align_items(AlignItems::CENTER)
        .padding(10.0)
        .background(col(45, 45, 60))
        .clip_rounded(18.0))
    .child((
        Icon(Symbols::PERSON).size(18.0).color(col(150, 150, 170)),
        Column(Modifier::new().flex_grow(1.0).gap(2.0)).child((
            RText(format!("Maker ID: {creator_short}"))
                .size(13.0)
                .color(RColor::WHITE),
            RText(if show_key {
                st.creator_recovery_key.clone()
            } else {
                "Recovery key: •••••• (hidden)".to_string()
            })
            .size(12.0)
            .color(if show_key {
                col(255, 217, 59)
            } else {
                col(140, 140, 160)
            }),
            RText(if st.creator_quota_text.is_empty() {
                "Weekly upload quota: -".to_string()
            } else {
                format!("{}", st.creator_quota_text)
            })
            .size(12.0)
            .color(col(150, 150, 170)),
        )),
        mk_icon_button(
            if show_key {
                Symbols::VISIBILITY_OFF
            } else {
                Symbols::VISIBILITY
            },
            true,
            move || {
                reveal_state.set(!reveal_state.get());
            },
        ),
        mk_icon_button(Symbols::COPY, true, move || {
            push(&a_copy_key, UiAction::CreatorCopyKey)
        }),
    ));

    // Import: paste a recovery key to "log in" with that identity here.
    let import_state: Rc<RefCell<TextFieldState>> =
        remember(|| RefCell::new(TextFieldState::new()));
    let import_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !import_focus.get() && import_state.borrow().text != st.creator_import_code {
        import_state.borrow_mut().text = st.creator_import_code.clone();
    }
    let a_import_submit = actions.clone();
    let a_import_go = actions.clone();
    let import_field = BasicTextField(
        import_state.clone(),
        Modifier::new().flex_grow(1.0).height(34.0),
        "Paste a recovery key (restore identity on this device)",
        TextFieldConfig {
            line_limits: TextFieldLineLimits::SingleLine,
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Text,
                ime_action: ImeAction::Done,
                ..KeyboardOptions::DEFAULT
            },
            on_submit: Some(Rc::new(move |v: String| {
                push(&a_import_submit, UiAction::CreatorImport(v))
            })),
            focus_tracker: Some(import_focus.clone()),
            ..Default::default()
        },
    );
    let import_row = Row(Modifier::new()
        .fill_max_width()
        .gap(8.0)
        .align_items(AlignItems::CENTER))
    .child((
        Row(Modifier::new()
            .fill_max_width()
            .gap(8.0)
            .align_items(AlignItems::CENTER)
            .padding(10.0)
            .background(col(45, 45, 60))
            .clip_rounded(18.0))
        .child((
            Icon(Symbols::KEY).size(16.0).color(col(150, 150, 170)),
            import_field,
        )),
        mk_pill_button(icon_label(Symbols::CHECK, "Import".into()), move || {
            let t = import_state.borrow().text.clone();
            push(&a_import_go, UiAction::CreatorImport(t));
        }),
    ));

    // Admin token: dev-only override, kept out of the way.
    let token_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let token_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !token_focus.get() && token_state.borrow().text != st.online_token {
        token_state.borrow_mut().text = st.online_token.clone();
    }
    let on_token_submit = {
        let a = a_submit_token.clone();
        Rc::new(move |v: String| push(&a, UiAction::OnlineSetToken(v))) as Rc<dyn Fn(String)>
    };
    let token_field = BasicTextField(
        token_state.clone(),
        Modifier::new().flex_grow(1.0).height(34.0),
        "Admin token (developer only)",
        TextFieldConfig {
            line_limits: TextFieldLineLimits::SingleLine,
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Text,
                ime_action: ImeAction::Done,
                ..KeyboardOptions::DEFAULT
            },
            on_submit: Some(on_token_submit),
            focus_tracker: Some(token_focus.clone()),
            ..Default::default()
        },
    );
    let token_row = Row(Modifier::new()
        .fill_max_width()
        .gap(8.0)
        .align_items(AlignItems::CENTER))
    .child((
        Row(Modifier::new()
            .fill_max_width()
            .gap(8.0)
            .align_items(AlignItems::CENTER)
            .padding(10.0)
            .background(col(45, 45, 60))
            .clip_rounded(18.0))
        .child((
            Icon(Symbols::LINK).size(16.0).color(col(150, 150, 170)),
            token_field,
        )),
        mk_pill_button(icon_label(Symbols::CHECK, "Set".into()), move || {
            let t = token_state.borrow().text.clone();
            push(&a_set_token, UiAction::OnlineSetToken(t));
        }),
    ));

    let advanced_open = remember_mutable(|| false);
    let adv_toggle = advanced_open.clone();
    let adv_import_focus = import_focus.clone();
    let adv_token_focus = token_focus.clone();
    let advanced_toggle = Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER))
    .child(mk_pill_button(
        icon_label(
            if *advanced_open.get() {
                Symbols::EXPAND_LESS
            } else {
                Symbols::EXPAND_MORE
            },
            "Account & developer".into(),
        ),
        move || {
            let next = !*adv_toggle.get();
            if !next {
                adv_import_focus.set(false);
                adv_token_focus.set(false);
            }
            adv_toggle.set(next);
        },
    ));
    let advanced_section = AnimatedVisibility(
        *advanced_open.get(),
        Column(Modifier::new().fill_max_width().gap(8.0)).child((import_row, token_row)),
        AnimatedVisibilityConfig {
            key: "online_advanced".into(),
            spec: AnimationSpec::tween(Duration::from_millis(200), Easing::EaseOut),
            ..Default::default()
        },
    );

    let verified_hint = RText(if st.level_verified {
        "Ready to publish".to_string()
    } else {
        "Beat the level to publish it".to_string()
    })
    .size(12.0)
    .color(if st.level_verified {
        col(90, 200, 120)
    } else {
        col(230, 160, 70)
    });

    let upload_button = mk_primary_button(
        icon_label(Symbols::CLOUD_UPLOAD, "Publish Current Level".into()),
        col(150, 110, 200),
        move || push(&a_upload, UiAction::OnlineUpload),
    );

    let sort_label = match st.online_sort % 4 {
        0 => "Sort: Newest",
        1 => "Sort: Name",
        2 => "Sort: Most liked",
        _ => "Sort: Most played",
    };
    let sort_text = RText(sort_label).size(13.0).color(col(190, 190, 205));
    let sort_button = FilledTonalButton(
        Modifier::new()
            .height(30.0)
            .padding(12.0)
            .background(col(45, 45, 60))
            .clip_rounded(15.0),
        move || push(&a_sort, UiAction::OnlineCycleSort),
        ButtonConfig::default(),
        move || sort_text.clone(),
    );
    let count_text: View = if st.online_loading {
        icon_text(
            Symbols::REFRESH,
            "Loading...".into(),
            12.0,
            col(230, 160, 70),
        )
    } else {
        RText(format!("{} levels", levels.len()))
            .size(12.0)
            .color(col(150, 150, 170))
    };
    let _sort_row: ();

    let shelf_labels = ["Fresh", "Popular", "Hot", "Mine"];
    let mut shelf_row_children: Vec<View> = Vec::new();
    for (i, name) in shelf_labels.iter().enumerate() {
        let a = actions.clone();
        let idx = i as u8;
        let selected = st.online_shelf == idx;
        shelf_row_children.push(InputChip(
            selected,
            move || push(&a, UiAction::OnlineSetShelf(idx)),
            RText(*name).size(13.0).color(if selected {
                RColor::WHITE
            } else {
                col(160, 160, 175)
            }),
            None,
            None,
            None,
            ChipConfig {
                shape_radius: 10.0,
                ..Default::default()
            },
        ));
    }
    let shelf_row = Row(Modifier::new().gap(6.0)).child(shelf_row_children);

    let id_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let id_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !id_focus.get() && id_state.borrow().text != st.online_id_query {
        id_state.borrow_mut().text = st.online_id_query.clone();
    }
    let id_change = actions.clone();
    let id_submit = actions.clone();
    let a_id_go = actions.clone();
    let id_field = BasicTextField(
        id_state.clone(),
        Modifier::new().width(170.0).height(34.0),
        "Level ID",
        TextFieldConfig {
            line_limits: TextFieldLineLimits::SingleLine,
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Number,
                ime_action: ImeAction::Search,
                ..KeyboardOptions::DEFAULT
            },
            on_change: Some(Rc::new(move |v: String| {
                push(&id_change, UiAction::OnlineSetIdQuery(v))
            })),
            on_submit: Some(Rc::new(move |v: String| {
                push(&id_submit, UiAction::OnlineSetIdQuery(v));
                push(&id_submit, UiAction::OnlineSearchId);
            })),
            focus_tracker: Some(id_focus.clone()),
            ..Default::default()
        },
    );
    let id_row = Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
        id_field,
        FilledTonalButton(
            Modifier::new().height(36.0),
            move || push(&a_id_go, UiAction::OnlineSearchId),
            ButtonConfig::default(),
            move || {
                Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                    Icon(Symbols::SEARCH).size(16.0).color(RColor::WHITE),
                    RText("Go").size(13.0).color(RColor::WHITE),
                ))
            },
        ),
    ));

    // Tell the Bevy-side browser nav to stand down while any text field owns focus.
    let any_text_focus =
        query_focus.get() || token_focus.get() || id_focus.get() || import_focus.get();
    if matches!(st.overlay, OverlayMenu::Online) {
        push(&actions, UiAction::SetKeyboardCaptured(any_text_focus));
    }

    // A 3-across grid of identity-only cards.
    let mut grid_children: Vec<View> = Vec::new();
    if levels.is_empty() {
        grid_children.push(
            RText("No levels online yet. Publish one, or search / refresh to re-fetch.")
                .size(14.0)
                .color(col(180, 180, 190)),
        );
    } else {
        for m in levels {
            grid_children.push(online_card(m, st, &actions));
        }
    }

    let scroll_state = remember_scroll_state("online_list");
    let grid_view = repose_ui::Grid(
        3,
        Modifier::new().fill_max_width(),
        grid_children,
        10.0,
        10.0,
    );
    let scroll_list = ScrollArea(
        Modifier::new().fill_max_width().weight(1.0),
        scroll_state,
        grid_view,
    );

    // Detail panel: identity + actions, shown when a card is selected.
    let detail: View = match st
        .online_levels
        .iter()
        .find(|m| st.online_selected == Some(m.id))
    {
        Some(m) => online_detail_panel(m, st, &actions),
        None => BrowseSelectPanel(vec![
            RText("Select a level").size(14.0).color(col(150, 150, 170)),
        ]),
    };

    let inner = Column(
        Modifier::new()
            .fill_max_width()
            .width(980.0)
            .max_height(820.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(header)
    .child(spacer(12.0))
    .child(search_row)
    .child(spacer(8.0))
    .child(identity_box)
    .child(spacer(8.0))
    .child(advanced_toggle)
    .child(advanced_section)
    .child(spacer(12.0))
    .child(
        Row(Modifier::new().gap(12.0).align_items(AlignItems::CENTER)).child((
            upload_button,
            verified_hint,
            Column(Modifier::new().weight(1.0)),
            id_row,
        )),
    )
    .child(spacer(12.0))
    .child(
        Row(Modifier::new().gap(12.0).align_items(AlignItems::CENTER)).child((
            shelf_row,
            Column(Modifier::new().weight(1.0)),
            sort_button,
            count_text,
        )),
    )
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

/// An identity-only online card: generated preview when available, name, author,
/// stats. No actions on the tile; click selects it and opens the detail panel.
fn online_card(m: &LevelMeta, st: &SharedUi, actions: &Arc<Mutex<Vec<UiAction>>>) -> View {
    let b_sel = actions.clone();
    let a_preview = actions.clone();
    let id = m.id;
    let selected = st.online_selected == Some(m.id);

    // Lazy-preview request. This is intentionally data-driven, not stored
    // thumbnail-driven: download level data once, generate ThumbPreview locally.
    let needs_preview =
        !st.online_previews.contains_key(&m.id) && !st.online_preview_pending.contains(&m.id);
    if needs_preview {
        push(&a_preview, UiAction::OnlinePreview(m.id));
    }

    let preview = st.online_previews.get(&m.id).cloned();
    let pending = st.online_preview_pending.contains(&m.id);

    let card_config = selected_card_config(selected);

    clickable_outlined_card(
        move || push(&b_sel, UiAction::OnlineSelect(id)),
        Modifier::new().fill_max_width(),
        card_config,
        move || {
            let date: String = m.created_at.chars().take(10).collect();

            let preview_view: View = card_preview_box(
                Some(match preview {
                    Some(p) => thumb_grid_view(&p),
                    None => online_preview_placeholder(m.id, pending),
                }),
                92.0,
            );

            let children: Vec<View> = vec![
                preview_view,
                Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                    RText(m.name.clone()).size(15.0).color(RColor::WHITE),
                    RText(format!("#{}", m.id))
                        .size(11.0)
                        .color(col(130, 130, 150)),
                )),
                RText(if m.author.is_empty() {
                    "Unknown".to_string()
                } else {
                    m.author.clone()
                })
                .size(12.0)
                .color(col(150, 150, 170)),
                RText(format!("♥ {}  ▶ {} · {}", m.likes, m.plays, date))
                    .size(11.0)
                    .color(col(140, 140, 160)),
            ];

            Column(Modifier::new().gap(6.0).align_items(AlignItems::FLEX_START))
                .with_children(children)
        },
    )
}

/// Placeholder shown while an online card's preview is not available yet.
fn online_preview_placeholder(id: u64, pending: bool) -> View {
    let label = if pending {
        "Generating preview…".to_string()
    } else {
        format!("#{id}")
    };

    Column(
        Modifier::new()
            .fill_max_width()
            .height(92.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .background(col(30, 30, 42))
            .clip_rounded(8.0),
    )
    .child((
        Icon(Symbols::CLOUD).size(22.0).color(col(120, 120, 145)),
        spacer(4.0),
        RText(label).size(12.0).color(col(140, 140, 160)),
    ))
}

/// Right-side detail panel for a selected online level with its actions.
fn online_detail_panel(m: &LevelMeta, st: &SharedUi, actions: &Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_play = actions.clone();
    let a_like = actions.clone();
    let a_delete = actions.clone();
    let a_report = actions.clone();
    let id = m.id;

    let confirming = st.online_confirm_delete == Some(id);
    let a_confirm_cancel = actions.clone();

    let play_button = FilledTonalButton(
        Modifier::new().height(36.0).fill_max_width(),
        move || push(&a_play, UiAction::OnlinePlay(id)),
        ButtonConfig::default(),
        move || {
            Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                Icon(Symbols::PLAY_ARROW).size(18.0).color(RColor::WHITE),
                RText("Play").size(14.0).color(RColor::WHITE),
            ))
        },
    );

    let secondary_row: View = if confirming {
        Column(Modifier::new().gap(6.0)).child((
            RText("Delete this level permanently?")
                .size(12.0)
                .color(col(232, 120, 120)),
            Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                FilledTonalButton(
                    Modifier::new()
                        .height(34.0)
                        .background(col(160, 60, 60))
                        .clip_rounded(8.0),
                    {
                        let ac = a_confirm_cancel.clone();
                        move || push(&ac, UiAction::OnlineDelete(id))
                    },
                    ButtonConfig::default(),
                    move || RText("Confirm Delete").size(13.0).color(RColor::WHITE),
                ),
                FilledTonalButton(
                    Modifier::new().height(34.0),
                    {
                        let ac = a_confirm_cancel.clone();
                        move || push(&ac, UiAction::OnlineDeleteCancel)
                    },
                    ButtonConfig::default(),
                    move || RText("Cancel").size(13.0).color(RColor::WHITE),
                ),
            )),
        ))
    } else {
        Row(Modifier::new().gap(6.0)).child((
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_like, UiAction::OnlineLike(id)),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::THUMB_UP).size(16.0).color(RColor::WHITE),
                        RText("Like").size(13.0).color(RColor::WHITE),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_delete, UiAction::OnlineDelete(id)),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::DELETE).size(16.0).color(col(232, 120, 120)),
                        RText("Delete").size(13.0).color(col(232, 120, 120)),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_report, UiAction::OnlineReport(id)),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::FLAG).size(16.0).color(RColor::WHITE),
                        RText("Report").size(13.0).color(RColor::WHITE),
                    ))
                },
            ),
        ))
    };

    let action_row = Column(Modifier::new().gap(6.0)).child((play_button, secondary_row));

    let mut tag_pills: Vec<View> = Vec::new();
    for tag in &m.tags {
        tag_pills.push(
            Column(
                Modifier::new()
                    .padding(6.0)
                    .background(col(60, 60, 80))
                    .clip_rounded(8.0),
            )
            .child(RText(tag.clone()).size(11.0).color(col(170, 170, 190))),
        );
    }

    Column(
        Modifier::new()
            .width(280.0)
            .fill_max_height()
            .background(col(24, 24, 34))
            .clip_rounded(12.0)
            .padding(14.0)
            .gap(8.0),
    )
    .child((
        card_preview_box(
            Some(match st.online_previews.get(&m.id) {
                Some(p) => thumb_grid_view(p),
                None => online_preview_placeholder(m.id, st.online_preview_pending.contains(&m.id)),
            }),
            120.0,
        ),
        RText(m.name.clone()).size(19.0).color(RColor::WHITE),
        RText(if m.author.is_empty() {
            "Unknown".to_string()
        } else {
            m.author.clone()
        })
        .size(13.0)
        .color(col(150, 150, 170)),
        if m.tags.is_empty() {
            RText("").size(0.0)
        } else {
            Row(Modifier::new().gap(6.0)).child(tag_pills)
        },
        RText(format!(
            "{} likes · {} plays · {:.1} KB",
            m.likes,
            m.plays,
            m.size_bytes as f32 / 1024.0
        ))
        .size(12.0)
        .color(col(140, 140, 160)),
        action_row,
    ))
}

/// Renders an isometric preview as a grid of colored boxes (Repose primitives
/// only). `cell` is the px size of each box (4.0 = card thumbnails).
pub fn preview_thumb(p: &ThumbPreview, cell: f32) -> View {
    let mut rows: Vec<View> = Vec::with_capacity(p.rows);
    for r in 0..p.rows {
        let mut cells: Vec<View> = Vec::with_capacity(p.cols);
        for cidx in 0..p.cols {
            let px = p.cells[r * p.cols + cidx];
            cells.push(Column(
                Modifier::new()
                    .width(cell)
                    .height(cell)
                    .background(RColor::from_rgba(px[0], px[1], px[2], px[3])),
            ));
        }
        rows.push(Row(Modifier::new()).child(cells));
    }

    Column(
        Modifier::new()
            .clip_rounded(8.0)
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
            if selected { 2.0 } else { 1.0 },
            if selected {
                col(255, 217, 59)
            } else {
                RColor::from_rgba(255, 255, 255, 40)
            },
        )),
        shape_radius: 12.0,
        ..Default::default()
    }
}

/// Shared card face: a fixed-height box that centers either a generated
/// thumbnail or a placeholder (used by both browse and online cards/detail).
fn card_preview_box(content: Option<View>, height: f32) -> View {
    Column(
        Modifier::new()
            .fill_max_width()
            .height(height)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .background(col(30, 30, 42))
            .clip_rounded(8.0),
    )
    .child(content.unwrap_or_else(|| RText("…").size(14.0).color(col(120, 120, 145))))
}
