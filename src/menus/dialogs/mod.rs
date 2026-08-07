use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{
    AlignItems, AlignSelf, Color as RColor, JustifyContent, Modifier, remember,
};
use repose_core::{ImeAction, KeyboardOptions, KeyboardType, TextFieldLineLimits};
use repose_material::material3::{
    ButtonConfig, DropdownMenu, DropdownMenuConfig, DropdownMenuEntry, DropdownMenuItem,
    FilledTonalButton, MenuState,
};
use repose_ui::overlay::OverlayHandle;
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::{
    BasicTextField, Column, FlowRow, Row, Text as RText, TextFieldConfig, TextFieldState,
    TextStyle, ViewExt,
};

use crate::app::SharedUi;
use crate::maker::level::LevelTag;
use crate::menus::action::UiAction;
use crate::menus::components::{
    Symbols, icon_label, icon_text, mk_button, mk_button_sm, mk_chip, mk_pill_button,
    mk_primary_button, modal_shell, push, push_ui, spacer,
};
use crate::menus::style::{col, t, tag_color, tok};

/// Play-mode dialog showing a sign's text (mirrors MB64's message panel).
/// Dismissed by pressing I / Space / Escape (handled on the Bevy side in
/// `read_signs`) or by the Close button.
pub(crate) fn sign_dialog_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let lines = st.sign_dialog_lines.clone();
    let mut body: Vec<View> = vec![
        RText(t(tr, "sign-dialog-title", "Sign"))
            .size(26.0)
            .color(col(217, 184, 115)),
        spacer(12.0),
    ];
    for line in lines {
        body.push(RText(line).size(16.0).color(RColor::WHITE));
    }
    body.push(spacer(16.0));
    let a_close = actions.clone();
    body.push(mk_button(
        &t(tr, "sign-dialog-close", "Close (I)"),
        col(60, 140, 90),
        move || push(&a_close, UiAction::MakerCloseSignDialog),
    ));

    modal_shell(
        Column(
            Modifier::new()
                .width(440.0)
                .padding(24.0)
                .background(col(20, 20, 28))
                .clip_rounded(12.0)
                .align_items(AlignItems::CENTER),
        )
        .children(body),
    )
}

/// Edit-mode modal for editing a sign's text. The field keeps its own
/// `TextFieldState`; Save pushes the text back through a `UiCommand` so it is
/// undoable.
pub(crate) fn sign_editor_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let text_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !focus.get() && text_state.borrow().text != st.sign_editor_text {
        text_state.borrow_mut().text = st.sign_editor_text.clone();
    }
    // Keep Bevy-side hotkeys from firing while the field owns focus.
    push(&actions, UiAction::SetKeyboardCaptured(focus.get()));

    let field = BasicTextField(
        text_state.clone(),
        Modifier::new()
            .width(380.0)
            .height(150.0)
            .align_self(AlignSelf::CENTER),
        t(tr, "sign-editor-hint", "What does the sign say?"),
        TextFieldConfig {
            line_limits: TextFieldLineLimits::MultiLine {
                min_height_in_lines: 5,
                max_height_in_lines: 8,
            },
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Text,
                ime_action: ImeAction::Default,
                ..KeyboardOptions::DEFAULT
            },
            focus_tracker: Some(focus.clone()),
            ..Default::default()
        },
    );

    let a_save = actions.clone();
    let a_cancel = actions.clone();
    let ts_save = text_state.clone();
    modal_shell(
        Column(
            Modifier::new()
                .width(460.0)
                .padding(24.0)
                .background(col(20, 20, 28))
                .clip_rounded(12.0)
                .align_items(AlignItems::CENTER),
        )
        .child((
            RText(t(tr, "sign-editor-title", "Sign Text"))
                .size(26.0)
                .color(col(217, 184, 115)),
            spacer(12.0),
            field,
            spacer(12.0),
            Row(Modifier::new().gap(10.0)).children(vec![
                mk_pill_button(
                    RText(t(tr, "sign-editor-save", "Save")).size(16.0),
                    move || {
                        push(
                            &a_save,
                            UiAction::MakerInspSetSignText(ts_save.borrow().text.clone()),
                        )
                    },
                ),
                mk_pill_button(
                    RText(t(tr, "sign-editor-cancel", "Cancel")).size(16.0),
                    move || push(&a_cancel, UiAction::MakerInspCancelSignText),
                ),
            ]),
        )),
    )
}

pub(crate) fn level_select_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_back = actions.clone();

    let row = |i: u8, lvl: &crate::maker::campaign::CampaignLevelUi| -> View {
        let a = actions.clone();
        let title = lvl.title.clone();
        let teaches = lvl.teaches.clone();
        let status = if lvl.completed {
            match (lvl.best_time, lvl.best_deaths) {
                (Some(best), Some(d)) => icon_text(
                    Symbols::CHECK,
                    format!("{:.1}s · {} {}", best, d, t(tr, "maker-deaths", "deaths")),
                    12.0,
                    col(220, 210, 120),
                ),
                _ => RText(t(tr, "completed", "Completed"))
                    .size(12.0)
                    .color(col(220, 210, 120)),
            }
        } else {
            RText(t(tr, "uncleared", "Uncleared"))
                .size(12.0)
                .color(col(120, 125, 140))
        };
        Column(Modifier::new().align_items(AlignItems::CENTER)).child((
            mk_button(
                &format!("{}. {}", i + 1, title),
                col(70, 90, 120),
                move || push(&a, UiAction::PlayBundledLevel(i)),
            ),
            RText(teaches).size(12.0).color(col(160, 165, 180)),
            status,
            spacer(4.0),
        ))
    };

    let inner = Column(
        Modifier::new()
            .width(420.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "level-select-title", "Tutorial Levels"))
            .size(32.0)
            .color(RColor::WHITE),
        spacer(12.0),
    ));

    let mut inner = inner;
    for (i, lvl) in st.campaign_levels.iter().enumerate() {
        inner = inner.child(row(i as u8, lvl));
    }

    inner = inner.child(spacer(12.0)).child(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push(&a_back, UiAction::CloseOverlay),
    ));

    modal_shell(inner)
}

pub(crate) fn pause_overlay(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a1 = actions.clone();
    let a2 = actions.clone();
    let a3 = actions.clone();
    let tr = &st.translations;

    modal_shell(pause_panel(tr, a1, a2, a3))
}

fn pause_panel(
    tr: &std::collections::HashMap<String, String>,
    a1: Arc<Mutex<Vec<UiAction>>>,
    a2: Arc<Mutex<Vec<UiAction>>>,
    a3: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    Column(
        Modifier::new()
            .width(320.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "paused", "Paused"))
            .size(36.0)
            .color(RColor::WHITE),
        spacer(16.0),
        mk_button(&t(tr, "resume", "Resume"), col(60, 140, 90), move || {
            push(&a1, UiAction::Resume)
        }),
        mk_button(&t(tr, "settings", "Settings"), col(70, 70, 90), move || {
            push(&a2, UiAction::OpenSettings)
        }),
        mk_button(
            &t(tr, "quit-to-title", "Quit to Title"),
            col(180, 60, 60),
            move || push(&a3, UiAction::QuitToTitle),
        ),
    ))
}

pub(crate) fn settings_ui(
    overlay: OverlayHandle,
    st: &SharedUi,
    actions: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let a_m_down = actions.clone();
    let a_m_up = actions.clone();
    let a_s_down = actions.clone();
    let a_s_up = actions.clone();
    let a_mu_down = actions.clone();
    let a_mu_up = actions.clone();
    let a_save = actions.clone();
    let a_back = actions.clone();
    let master = st.master_vol;
    let sfx = st.sfx_vol;
    let music = st.music_vol;
    let tr = &st.translations;
    let lang = &st.language;
    let langs = &st.available_languages;
    let overlay_clone = overlay.clone();
    let actions_clone = actions.clone();

    let menu_state: Rc<MenuState> = remember(MenuState::new);
    let lang_items: Vec<DropdownMenuEntry> = langs
        .iter()
        .map(|l| {
            let a = actions_clone.clone();
            let code = l.clone();
            let mut item = DropdownMenuItem::new(l.clone(), move || {
                push(&a, UiAction::SetLanguage(code.clone()))
            });
            if l == lang {
                item = item.disabled();
            }
            DropdownMenuEntry::Item(item)
        })
        .collect();
    let menu_trigger = menu_state.clone();
    let lang_label = st.language.clone();
    let trigger = FilledTonalButton(
        Modifier::new().width(100.0).height(40.0),
        move || menu_trigger.open(),
        ButtonConfig::default(),
        move || RText(lang_label.clone()).size(20.0),
    );

    let lang_dropdown = DropdownMenu(
        menu_state,
        overlay_clone,
        Modifier::new(),
        trigger,
        lang_items,
        DropdownMenuConfig {
            min_width: 100.0,
            ..Default::default()
        },
    );

    let inner = Column(
        Modifier::new()
            .width(360.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "settings", "Settings"))
            .size(36.0)
            .color(RColor::WHITE),
    )
    .child(spacer(12.0))
    .child(
        RText(format!(
            "{}: {:.0}%",
            t(tr, "master-volume", "Master"),
            master * 100.0
        ))
        .size(18.0)
        .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(8.0)).child((
        mk_button_sm("-", move || {
            push(&a_m_down, UiAction::SetMasterVol(master - 0.1))
        }),
        mk_button_sm("+", move || {
            push(&a_m_up, UiAction::SetMasterVol(master + 0.1))
        }),
    )))
    .child(spacer(8.0))
    .child(
        RText(format!(
            "{}: {:.0}%",
            t(tr, "sfx-volume", "SFX"),
            sfx * 100.0
        ))
        .size(18.0)
        .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(8.0)).child((
        mk_button_sm("-", move || push(&a_s_down, UiAction::SetSfxVol(sfx - 0.1))),
        mk_button_sm("+", move || push(&a_s_up, UiAction::SetSfxVol(sfx + 0.1))),
    )))
    .child(spacer(8.0))
    .child(
        RText(format!(
            "{}: {:.0}%",
            t(tr, "music-volume", "Music"),
            music * 100.0
        ))
        .size(18.0)
        .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(8.0)).child((
        mk_button_sm("-", move || {
            push(&a_mu_down, UiAction::SetMusicVol(music - 0.1))
        }),
        mk_button_sm("+", move || {
            push(&a_mu_up, UiAction::SetMusicVol(music + 0.1))
        }),
    )))
    .child(spacer(8.0))
    .child(
        RText(format!("{}:", t(tr, "language", "Language")))
            .size(18.0)
            .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(6.0)).child(lang_dropdown))
    .child(spacer(16.0))
    .child(mk_button(
        &t(tr, "save", "Save"),
        col(60, 120, 200),
        move || push(&a_save, UiAction::SaveSettings),
    ))
    .child(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push(&a_back, UiAction::CloseOverlay),
    ));

    modal_shell(inner)
}

pub(crate) fn credits_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a = actions.clone();
    let tr = &st.translations;
    let inner = Column(
        Modifier::new()
            .width(400.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "credits", "Credits"))
            .size(36.0)
            .color(RColor::WHITE),
        spacer(12.0),
        RText("Original Godot template: mlm-games")
            .size(16.0)
            .color(RColor::WHITE),
        RText("Bevy + Repose port: mlm-games")
            .size(16.0)
            .color(RColor::WHITE),
        RText("Engine: Bevy  UI: Repose")
            .size(16.0)
            .color(RColor::WHITE),
        RText("3D models: Quaternius Cubeworld Kit (CC0)")
            .size(16.0)
            .color(RColor::WHITE),
        spacer(16.0),
        mk_button(&t(tr, "back", "Back"), col(70, 70, 90), move || {
            push(&a, UiAction::CloseOverlay)
        }),
    ));

    modal_shell(inner)
}

pub(crate) fn level_clear_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_edit = actions.clone();
    let a_retry = actions.clone();
    let a_remix = actions.clone();
    let a_menu = actions.clone();

    let mut body: Vec<View> = vec![
        RText(t(tr, "maker-clear-title", "Level Clear!"))
            .size(38.0)
            .color(RColor::WHITE),
        spacer(12.0),
        RText(if st.level_verified {
            t(tr, "maker-clear-verified", "Level Verified!")
        } else {
            String::new()
        })
        .size(20.0)
        .color(col(90, 200, 120)),
        spacer(6.0),
        RText(format!(
            "{}: {:.2}s",
            t(tr, "maker-time", "Time"),
            st.clear_time_secs
        ))
        .size(18.0)
        .color(RColor::WHITE),
    ];

    if st.player_is_author {
        body.push(icon_text(
            Symbols::STAR,
            format!(
                "{} ({:.2}s)",
                t(tr, "maker-cleared", "Cleared!"),
                st.clear_time_secs
            ),
            16.0,
            col(120, 230, 140),
        ));
    } else if st.new_record {
        body.push(icon_text(
            Symbols::STAR,
            format!(
                "{} ({:.2}s)",
                t(tr, "maker-new-record", "New record!"),
                st.clear_time_secs
            ),
            16.0,
            col(120, 230, 140),
        ));
    } else if let Some(record) = st.record_ms {
        body.push(icon_text(
            Symbols::STAR,
            format!(
                "{}: {:.2}s",
                t(tr, "maker-record", "Record"),
                record as f32 / 1000.0
            ),
            16.0,
            col(255, 200, 90),
        ));
    } else if st.first_clear {
        body.push(icon_text(
            Symbols::STAR,
            format!(
                "{} ({:.2}s)",
                t(tr, "maker-first-clear", "First clear!"),
                st.clear_time_secs
            ),
            16.0,
            col(120, 230, 140),
        ));
    }

    body.push(
        RText(format!(
            "{}: {}",
            t(tr, "maker-deaths", "Deaths"),
            st.clear_deaths
        ))
        .size(18.0)
        .color(RColor::WHITE),
    );
    body.push(
        RText(format!(
            "{}: {}/{}",
            t(tr, "maker-glimmers-count", "Glimmers"),
            st.glimmers_collected,
            st.glimmers_total
        ))
        .size(18.0)
        .color(col(255, 220, 100)),
    );
    body.push(
        RText(format!(
            "{}: {}",
            t(tr, "maker-blocks-count", "Blocks"),
            st.blocks_placed
        ))
        .size(18.0)
        .color(RColor::WHITE),
    );

    body.push(spacer(16.0));
    // Bundled levels are read-only sources: Remix instead of editing in place.
    if st.is_bundled {
        body.push(mk_button(
            &t(tr, "maker-remix", "Remix This Level"),
            col(150, 100, 220),
            move || push_ui(&a_remix, UiAction::MakerRemix),
        ));
    } else {
        body.push(mk_button(
            &t(tr, "maker-btn-edit", "Edit Level"),
            col(70, 110, 170),
            move || push_ui(&a_edit, UiAction::MakerDismissClear),
        ));
    }
    body.push(mk_button(
        &t(tr, "maker-retry", "Retry"),
        col(60, 140, 90),
        move || push_ui(&a_retry, UiAction::MakerRetry),
    ));
    body.push(mk_button(
        &t(tr, "back-to-menu", "Back to Menu"),
        col(180, 60, 60),
        move || push_ui(&a_menu, UiAction::QuitToTitle),
    ));

    let inner = Column(
        Modifier::new()
            .width(380.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .children(body);

    modal_shell(inner)
}

pub(crate) fn load_level_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_back = actions.clone();

    let mut slot_views: Vec<View> = Vec::new();
    if st.level_slots.is_empty() {
        slot_views.push(
            RText(t(tr, "maker-load-empty", "No saved levels"))
                .size(16.0)
                .color(col(180, 180, 190)),
        );
    } else {
        for name in st.level_slots.iter().take(12) {
            let a = actions.clone();
            let n = name.clone();
            let label = name.clone();
            slot_views.push(mk_button(&label, col(70, 70, 90), move || {
                push_ui(&a, UiAction::MakerLoadSlot(n.clone()))
            }));
        }
    }

    let inner = Column(
        Modifier::new()
            .width(380.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "maker-load-title", "Load Level"))
            .size(32.0)
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(if st.level_name.is_empty() {
            String::new()
        } else {
            format!("{}: {}", t(tr, "maker-current", "Current"), st.level_name)
        })
        .size(14.0)
        .color(col(180, 180, 190)),
    )
    .child(spacer(12.0))
    .child(slot_views)
    .child(spacer(12.0))
    .child(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push_ui(&a_back, UiAction::CloseOverlay),
    ));

    modal_shell(inner)
}

pub(crate) fn share_overlay_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_close = actions.clone();
    let a_copy = actions.clone();
    let a_export = actions.clone();
    let a_import = actions.clone();
    let a_save = actions.clone();
    let import_code = st.import_code.clone();

    let mut tail: Vec<View> = Vec::new();
    if !st.is_bundled {
        tail.push(mk_primary_button(
            RText("Save to My Collection"),
            col(150, 110, 200),
            move || push(&a_save, UiAction::BrowseAddToCollection),
        ));
    }
    tail.push(spacer(16.0));
    tail.push(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push_ui(&a_close, UiAction::CloseOverlay),
    ));

    let inner = Column(
        Modifier::new()
            .width(520.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "share-title", "Share Level"))
            .size(32.0)
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(if st.level_verified {
            t(tr, "share-verified", "Verified")
        } else {
            t(tr, "share-unverified", "Beat the level to share it")
        })
        .size(14.0)
        .color(if st.level_verified {
            col(90, 200, 120)
        } else {
            col(230, 160, 70)
        }),
    )
    .child(spacer(16.0))
    .child(
        RText(t(tr, "share-export-title", "Export"))
            .size(18.0)
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(if st.export_code.is_empty() {
            t(tr, "share-export-empty", "No code yet")
        } else {
            st.export_code.clone()
        })
        .size(14.0)
        .color(if st.export_code.is_empty() {
            col(180, 180, 190)
        } else {
            col(120, 200, 255)
        }),
    )
    .child(spacer(4.0))
    .child(
        RText(st.export_error.clone().unwrap_or_default())
            .size(13.0)
            .color(col(230, 110, 110)),
    )
    .child(spacer(8.0))
    .child(mk_button(
        &t(tr, "share-export", "Generate Code"),
        col(70, 110, 170),
        move || push_ui(&a_export, UiAction::MakerExportCode),
    ))
    .child(mk_button(
        &t(tr, "share-copy", "Copy Code"),
        col(60, 140, 90),
        move || push_ui(&a_copy, UiAction::MakerCopyCode),
    ))
    .child(spacer(20.0))
    .child(
        RText(t(tr, "share-import-title", "Import"))
            .size(18.0)
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(if st.import_code.is_empty() {
            t(
                tr,
                "share-import-hint",
                "Type or paste a code, then press Enter",
            )
        } else {
            st.import_code.clone()
        })
        .size(14.0)
        .color(if st.import_code.is_empty() {
            col(180, 180, 190)
        } else {
            col(120, 200, 255)
        }),
    )
    .child(spacer(8.0))
    .child(mk_button(
        &t(tr, "share-import", "Import"),
        col(160, 120, 60),
        move || push_ui(&a_import, UiAction::MakerImportCode(import_code.clone())),
    ))
    .child(spacer(8.0))
    .child(tail);

    modal_shell(inner)
}

pub(crate) fn level_info_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_close = actions.clone();
    let a_save = actions.clone();
    let a_focus = actions.clone();

    let field = |label: &'static str, value: String, focus: u8| -> View {
        let a = a_focus.clone();
        let focused = st.info_focus == focus;
        let display = if value.is_empty() {
            "-".to_string()
        } else {
            value.clone()
        };
        FilledTonalButton(
            Modifier::new()
                .fill_max_width()
                .padding(10.0)
                .background(if focused {
                    col(70, 90, 120)
                } else {
                    col(45, 45, 60)
                })
                .clip_rounded(8.0),
            move || push(&a, UiAction::LevelInfoFocus(focus)),
            ButtonConfig::default(),
            move || {
                Column(Modifier::new().align_items(AlignItems::FLEX_START)).child((
                    RText(label).size(11.0).color(col(150, 150, 170)),
                    RText(display.clone()).size(14.0).color(RColor::WHITE),
                ))
            },
        )
    };

    let mut tag_views: Vec<View> = Vec::new();
    for tag in LevelTag::ALL {
        let a = actions.clone();
        let included = st.info_tags.contains(&tag);
        let label = RText(tag.label()).size(12.0).color(if included {
            RColor::WHITE
        } else {
            col(150, 150, 170)
        });
        tag_views.push(mk_chip(label, included, tag_color(tag), move || {
            push(&a, UiAction::LevelInfoToggleTag(tag))
        }));
    }
    let tag_row = FlowRow(Modifier::new().fill_max_width().gap(6.0)).child(tag_views);

    // Level Settings: boundary preset, water plane.
    let mut preset_views: Vec<View> = Vec::new();
    for p in crate::maker::level::BoundaryPreset::ALL {
        let a = actions.clone();
        let selected = st.info_preset == Some(p);
        let label = RText(p.label()).size(11.0).color(RColor::WHITE);
        preset_views.push(mk_chip(label, selected, col(90, 90, 120), move || {
            push(&a, UiAction::LevelInfoPreset(p))
        }));
    }
    let preset_row = FlowRow(Modifier::new().fill_max_width().gap(6.0)).child(preset_views);

    // Water plane.
    let a_wup = actions.clone();
    let a_wdn = actions.clone();
    let water_label = RText(match st.info_water {
        Some(level) => "  Water y = ".to_string() + &level.to_string() + "  ",
        None => "  No water  ".to_string(),
    });
    let water_minus = mk_button_sm("-", move || push(&a_wdn, UiAction::LevelInfoWaterDelta(-1)));
    let water_plus = mk_button_sm("+", move || push(&a_wup, UiAction::LevelInfoWaterDelta(1)));
    let settings_row = Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER))
        .child(water_minus)
        .child(water_label)
        .child(water_plus);

    // Size control (radius half-extents rx × rz wide, ry tall).
    let a_sup = actions.clone();
    let a_sdn = actions.clone();
    let size_auto = st.info_size_auto;
    let size_label = RText(if size_auto {
        "  Size: auto  ".to_string()
    } else {
        format!(
            "  Size {}×{}×{}  ",
            st.info_size[0], st.info_size[1], st.info_size[2]
        )
    });
    let size_minus = mk_button_sm("-", move || push(&a_sdn, UiAction::LevelInfoSizeDelta(-1)));
    let size_plus = mk_button_sm("+", move || push(&a_sup, UiAction::LevelInfoSizeDelta(1)));
    let a_sauto = actions.clone();
    let size_auto_btn = mk_button_sm("Auto", move || {
        push_ui(&a_sauto, UiAction::LevelInfoSizeAuto)
    });
    let size_row = Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER))
        .child(size_minus)
        .child(size_label)
        .child(size_plus)
        .child(size_auto_btn);

    // Boundary / room height (independent of the x/z size).
    let a_hup = actions.clone();
    let a_hdn = actions.clone();
    let a_hauto = actions.clone();
    let height_label = RText(if st.info_height == 0 {
        "  Height: auto  ".to_string()
    } else {
        format!("  Height: {} cells  ", st.info_height)
    });
    let height_minus = mk_button_sm("-", move || {
        push(&a_hdn, UiAction::LevelInfoHeightDelta(-1))
    });
    let height_plus = mk_button_sm("+", move || push(&a_hup, UiAction::LevelInfoHeightDelta(1)));
    let height_auto_btn = mk_button_sm("Auto", move || {
        push_ui(&a_hauto, UiAction::LevelInfoHeightAuto)
    });
    let height_row = Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER))
        .child(height_minus)
        .child(height_label)
        .child(height_plus)
        .child(height_auto_btn);

    let cond_label = match st.info_clear_condition {
        crate::maker::level::ClearCondition::ReachGoal => "Reach Goal".to_string(),
        crate::maker::level::ClearCondition::CollectAllGlimmers => {
            "Collect All Glimmers".to_string()
        }
        crate::maker::level::ClearCondition::DefeatAllProwlers => "Defeat All Prowlers".to_string(),
        crate::maker::level::ClearCondition::NoDeath => "No Death".to_string(),
        crate::maker::level::ClearCondition::TimeLimitMs(ms) => {
            format!("Time Limit · {}:{:02}", ms / 60_000, (ms / 1_000) % 60)
        }
    };

    let a_cond = actions.clone();
    let condition_row =
        Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER)).child(mk_pill_button(
            RText(format!("Condition: {cond_label}"))
                .size(12.0)
                .color(RColor::WHITE),
            move || push(&a_cond, UiAction::LevelInfoCycleClearCondition),
        ));

    let limit_label = match st.info_clear_condition {
        crate::maker::level::ClearCondition::TimeLimitMs(ms) => {
            format!(
                "  {}:{:02}.{:03}  ",
                ms / 60_000,
                (ms / 1_000) % 60,
                ms % 1_000
            )
        }
        _ => "  Only used by Time Limit  ".to_string(),
    };
    let a_tup = actions.clone();
    let a_tdn = actions.clone();
    let time_row = Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER))
        .child(mk_button_sm("-", move || {
            push(&a_tdn, UiAction::LevelInfoTimeLimitDelta(-15))
        }))
        .child(RText(limit_label).size(12.0).color(col(200, 200, 210)))
        .child(mk_button_sm("+", move || {
            push(&a_tup, UiAction::LevelInfoTimeLimitDelta(15))
        }));

    let stats = format!(
        "Blocks: {}   ·   Entities: {}",
        st.info_blocks, st.info_entities
    );

    // Everything between the title and the footer buttons lives in a scroll
    // area so the dialog fits short displays instead of clipping off-screen.
    let scroll_state = remember_scroll_state("level_info");
    let body = Column(
        Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER),
    )
    .child(field("Name", st.info_name.clone(), 0))
    .child(spacer(8.0))
    .child(field("Author", st.info_author.clone(), 1))
    .child(spacer(8.0))
    .child(field("Description", st.info_description.clone(), 2))
    .child(spacer(16.0))
    .child(RText("Tags").size(14.0).color(col(180, 180, 190)))
    .child(spacer(6.0))
    .child(tag_row)
    .child(spacer(16.0))
    .child(RText("Level Settings").size(14.0).color(col(180, 180, 190)))
    .child(spacer(6.0))
    .child(
        RText("Boundary presets: floor caught, walls rim, ceiling cap.")
            .size(11.0)
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(preset_row)
    .child(spacer(8.0))
    .child(settings_row)
    .child(spacer(8.0))
    .child(
        RText("Cycle the level’s win rule. Time limit uses ±15s below.")
            .size(11.0)
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(condition_row)
    .child(spacer(8.0))
    .child(time_row)
    .child(spacer(12.0))
    .child(
        RText("Type size: shrink from auto box, grow to enlarge.")
            .size(11.0)
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(size_row)
    .child(spacer(8.0))
    .child(
        RText("Height is the room/wall top; the ceiling sits there.")
            .size(11.0)
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(height_row)
    .child(spacer(12.0))
    .child(RText(stats).size(13.0).color(col(200, 200, 210)))
    .child(spacer(12.0))
    .child(
        RText("Saving metadata does not reset verification.")
            .size(11.0)
            .color(col(130, 130, 150)),
    );

    let scroll = ScrollArea(
        Modifier::new().fill_max_width().max_height(460.0),
        scroll_state,
        body,
    );

    let inner = Column(
        Modifier::new()
            .width(480.0)
            .max_height(720.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(RText("Level Info").size(32.0).color(RColor::WHITE))
    .child(spacer(6.0))
    .child(
        RText("Click a field to edit · tags show up in Browse")
            .size(12.0)
            .color(col(150, 150, 170)),
    )
    .child(spacer(16.0))
    .child(scroll)
    .child(spacer(16.0))
    .child(mk_primary_button(
        icon_label(Symbols::SAVE, "Save".into()),
        col(60, 140, 90),
        move || push(&a_save, UiAction::LevelInfoSave),
    ))
    .child(mk_button("Back", col(70, 70, 90), move || {
        push(&a_close, UiAction::LevelInfoClose)
    }));

    modal_shell(inner)
}

// Shared pieces

pub(crate) fn toast_anchor(st: &SharedUi) -> View {
    if st.maker_status.is_empty() {
        return Column(Modifier::new());
    }
    Column(
        Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::FLEX_START)
            .justify_content(JustifyContent::FLEX_END)
            .padding(16.0),
    )
    .child(
        Column(
            Modifier::new()
                .padding(10.0)
                .background(tok::bg_panel())
                .clip_rounded(8.0),
        )
        .child(
            RText(st.maker_status.clone())
                .size(14.0)
                .color(RColor::WHITE),
        ),
    )
}
