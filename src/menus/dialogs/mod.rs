use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use repose_core::prelude::{
    AlignItems, AlignSelf, Color as RColor, Dp, Modifier, Sp, remember, remember_with_key,
};
use repose_core::{ImeAction, KeyboardOptions, KeyboardType, TextFieldLineLimits, View};
use repose_material::material3::{Slider, SliderConfig};
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::{
    BasicTextField, Column, FlowRow, FlowRowConfig, Row, Text as RText, TextFieldConfig,
    TextFieldState, TextStyle, ViewExt,
};

use crate::maker::level::{BoundaryPreset, ClearCondition, LevelTag};
use crate::menus::MenuState;
use crate::menus::OverlayMenu;
use crate::menus::action::UiAction;
use crate::menus::components::{
    Symbols, icon_label, icon_text, mk_button, mk_button_sm, mk_chip, mk_pill_button,
    mk_primary_button, modal_shell, push, spacer,
};
use crate::menus::style::{col, t, tag_color};

/// Play-mode dialog showing a sign's text (mirrors MB64's message panel).
/// Dismissed by pressing I / Space / Escape (handled in the interaction
/// systems) or by the Close button.
pub(crate) fn sign_dialog_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let lines = st.sign_dialog_lines.clone();
    let mut body: Vec<View> = vec![
        RText(t(tr, "sign-dialog-title", "Sign"))
            .size(Sp(26.0))
            .color(col(217, 184, 115)),
        spacer(12.0),
    ];
    for line in lines {
        body.push(RText(line).size(Sp(16.0)).color(RColor::WHITE));
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
                .width(Dp(440.0))
                .padding(Dp(24.0))
                .background(col(20, 20, 28))
                .clip_rounded(Dp(12.0))
                .align_items(AlignItems::CENTER),
        )
        .children(body),
    )
}

pub(crate) fn pause_overlay(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a1 = actions.clone();
    let a3 = actions.clone();
    let a_retry = actions.clone();
    let a_settings = actions.clone();
    let a_share = actions.clone();
    let tr = &st.translations;

    modal_shell(pause_panel(tr, st, a1, a_settings, a_share, a3, a_retry))
}

fn pause_panel(
    tr: &std::collections::HashMap<String, String>,
    st: &MenuState,
    a1: Arc<Mutex<Vec<UiAction>>>,
    a_settings: Arc<Mutex<Vec<UiAction>>>,
    a_share: Arc<Mutex<Vec<UiAction>>>,
    a3: Arc<Mutex<Vec<UiAction>>>,
    a_retry: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let mut children: Vec<View> = vec![
        RText(t(tr, "paused", "Paused"))
            .size(Sp(36.0))
            .color(RColor::WHITE),
        spacer(16.0),
        mk_button(&t(tr, "resume", "Resume"), col(60, 140, 90), move || {
            push(&a1, UiAction::Resume)
        }),
    ];
    // Retry is only meaningful in Play; in Edit there is nothing to reset.
    // It lives here (not on the locked Play HUD) because `cursor_policy`
    // unlocks the mouse while paused, so it is actually clickable. During
    // unpaused Play use `R`.
    if !st.maker_mode_edit {
        children.push(mk_button(
            &t(tr, "maker-retry", "Retry"),
            col(90, 140, 200),
            move || push(&a_retry, UiAction::MakerRetry),
        ));
    }
    children.push(mk_button(
        &t(tr, "settings", "Settings"),
        col(70, 70, 90),
        move || push(&a_settings, UiAction::OpenSettings),
    ));
    if st.maker_mode_edit {
        children.push(mk_button(
            &t(tr, "share-title", "Share"),
            col(110, 90, 160),
            move || push(&a_share, UiAction::MakerPublish),
        ));
    }
    children.push(mk_button(
        &t(tr, "quit-to-title", "Quit to Title"),
        col(180, 60, 60),
        move || push(&a3, UiAction::QuitToTitle),
    ));

    Column(
        Modifier::new()
            .width(Dp(320.0))
            .padding(Dp(24.0))
            .background(col(20, 20, 28))
            .clip_rounded(Dp(12.0))
            .align_items(AlignItems::CENTER),
    )
    .children(children)
}

/// Edit-mode sign text editor opened from the entity inspector.
pub(crate) fn sign_editor_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let text_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !focus.get() && text_state.borrow().text != st.sign_editor_text {
        text_state.borrow_mut().text = st.sign_editor_text.clone();
    }
    // Keep the window-level hotkeys from firing while the field owns focus.
    push(&actions, UiAction::SetKeyboardCaptured(focus.get()));

    let field = BasicTextField(
        text_state.clone(),
        Modifier::new()
            .width(Dp(380.0))
            .height(Dp(150.0))
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
                .width(Dp(460.0))
                .padding(Dp(24.0))
                .background(col(20, 20, 28))
                .clip_rounded(Dp(12.0))
                .align_items(AlignItems::CENTER),
        )
        .child((
            RText(t(tr, "sign-editor-title", "Sign Text"))
                .size(Sp(26.0))
                .color(col(217, 184, 115)),
            spacer(12.0),
            field,
            spacer(12.0),
            repose_ui::Row(Modifier::new().gap(Dp(10.0))).children(vec![
                mk_pill_button(
                    RText(t(tr, "sign-editor-save", "Save")).size(Sp(16.0)),
                    move || {
                        push(
                            &a_save,
                            UiAction::MakerInspSetSignText(ts_save.borrow().text.clone()),
                        )
                    },
                ),
                mk_pill_button(
                    RText(t(tr, "sign-editor-cancel", "Cancel")).size(Sp(16.0)),
                    move || push(&a_cancel, UiAction::MakerInspCancelSignText),
                ),
            ]),
        )),
    )
}

/// In-game load panel: pick a named local slot to replace the session level.
pub(crate) fn load_level_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_back = actions.clone();

    let mut slot_views: Vec<View> = Vec::new();
    if st.level_slots.is_empty() {
        slot_views.push(
            RText(t(tr, "maker-load-empty", "No saved levels"))
                .size(Sp(16.0))
                .color(col(180, 180, 190)),
        );
    } else {
        for name in st.level_slots.iter().take(12) {
            let a = actions.clone();
            let n = name.clone();
            let label = name.clone();
            slot_views.push(mk_button(&label, col(70, 70, 90), move || {
                push(&a, UiAction::MakerLoadSlot(n.clone()))
            }));
        }
    }

    let inner = Column(
        Modifier::new()
            .width(Dp(380.0))
            .padding(Dp(24.0))
            .background(col(20, 20, 28))
            .clip_rounded(Dp(12.0))
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "maker-load-title", "Load Level"))
            .size(Sp(32.0))
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(if st.level_name.is_empty() {
            String::new()
        } else {
            format!("{}: {}", t(tr, "maker-current", "Current"), st.level_name)
        })
        .size(Sp(14.0))
        .color(col(180, 180, 190)),
    )
    .child(spacer(12.0))
    .child(slot_views)
    .child(spacer(12.0))
    .child(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push(&a_back, UiAction::CloseOverlay),
    ));

    modal_shell(inner)
}

pub(crate) fn settings_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_save = actions.clone();
    let a_back = actions.clone();

    let vol_row = |label: String, value: f32, mk: fn(f32) -> UiAction| -> View {
        let a = actions.clone();
        Column(Modifier::new().fill_max_width().gap(Dp(6.0)))
            .child(
                RText(format!("{label} · {:.0}%", value * 100.0))
                    .size(Sp(14.0))
                    .color(RColor::WHITE),
            )
            .child(Slider(
                value,
                (0.0, 1.0),
                Some(0.05),
                move |v| push(&a, mk(v)),
                SliderConfig {
                    modifier: Modifier::new().fill_max_width(),
                    ..Default::default()
                },
            ))
    };

    let mut lang_chips: Vec<View> = Vec::new();
    for lang in &st.available_languages {
        let a = actions.clone();
        let code = lang.clone();
        let selected = st.language == *lang;
        lang_chips.push(mk_chip(
            RText(lang.clone()).size(Sp(13.0)).color(if selected {
                RColor::WHITE
            } else {
                col(150, 150, 170)
            }),
            selected,
            col(60, 120, 200),
            move || push(&a, UiAction::SetLanguage(code.clone())),
        ));
    }
    let lang_row = FlowRow(
        Modifier::new().fill_max_width().gap(Dp(6.0)),
        FlowRowConfig::default(),
    )
    .child(lang_chips);

    let inner = Column(
        Modifier::new()
            .width(Dp(420.0))
            .padding(Dp(24.0))
            .background(col(20, 20, 28))
            .clip_rounded(Dp(12.0))
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "settings", "Settings"))
            .size(Sp(32.0))
            .color(RColor::WHITE),
    )
    .child(spacer(16.0))
    .child(vol_row(
        t(tr, "master-volume", "Master Volume"),
        st.master_vol,
        UiAction::SetMasterVol,
    ))
    .child(spacer(12.0))
    .child(vol_row(
        t(tr, "sfx-volume", "SFX Volume"),
        st.sfx_vol,
        UiAction::SetSfxVol,
    ))
    .child(spacer(12.0))
    .child(vol_row(
        t(tr, "music-volume", "Music Volume"),
        st.music_vol,
        UiAction::SetMusicVol,
    ))
    .child(spacer(16.0))
    .child(
        RText(t(tr, "language", "Language"))
            .size(Sp(15.0))
            .color(RColor::WHITE),
    )
    .child(spacer(6.0))
    .child(lang_row)
    .child(spacer(20.0))
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

pub(crate) fn credits_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a = actions.clone();
    let tr = &st.translations;
    let inner = Column(
        Modifier::new()
            .width(Dp(400.0))
            .padding(Dp(24.0))
            .background(col(20, 20, 28))
            .clip_rounded(Dp(12.0))
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "credits", "Credits"))
            .size(Sp(36.0))
            .color(RColor::WHITE),
        spacer(12.0),
        RText("Original Godot template: mlm-games")
            .size(Sp(16.0))
            .color(RColor::WHITE),
        RText("Repame + Repose port: mlm-games")
            .size(Sp(16.0))
            .color(RColor::WHITE),
        RText("Engine: Repame  UI: Repose")
            .size(Sp(16.0))
            .color(RColor::WHITE),
        RText("3D models: Cube World by Quaternius (CC0)")
            .size(Sp(16.0))
            .color(RColor::WHITE),
        spacer(16.0),
        mk_button(&t(tr, "back", "Back"), col(70, 70, 90), move || {
            push(&a, UiAction::CloseOverlay)
        }),
    ));

    modal_shell(inner)
}

pub(crate) fn share_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_close = actions.clone();
    let a_copy = actions.clone();
    let a_export = actions.clone();
    let a_import = actions.clone();
    let a_collection = actions.clone();

    let import_state: Rc<RefCell<TextFieldState>> =
        remember(|| RefCell::new(TextFieldState::new()));
    let import_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if matches!(st.overlay, OverlayMenu::Share) {
        push(&actions, UiAction::SetKeyboardCaptured(import_focus.get()));
    }
    let field = BasicTextField(
        import_state.clone(),
        Modifier::new().fill_max_width(),
        t(
            tr,
            "share-import-hint",
            "Type or paste a code, then press Enter",
        ),
        TextFieldConfig {
            line_limits: TextFieldLineLimits::SingleLine,
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Filter,
                ime_action: ImeAction::Go,
                ..KeyboardOptions::DEFAULT
            },
            on_submit: Some({
                let a = a_import.clone();
                Rc::new(move |v: String| push(&a, UiAction::MakerImportCode(v)))
                    as Rc<dyn Fn(String)>
            }),
            focus_tracker: Some(import_focus.clone()),
            ..Default::default()
        },
    );

    let mut tail: Vec<View> = Vec::new();
    if !st.is_bundled {
        tail.push(mk_primary_button(
            RText("Save to My Collection").size(Sp(15.0)),
            col(150, 110, 200),
            move || push(&a_collection, UiAction::BrowseAddToCollection),
        ));
        tail.push(spacer(16.0));
    }
    tail.push(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push(&a_close, UiAction::CloseOverlay),
    ));

    let inner = Column(
        Modifier::new()
            .width(Dp(520.0))
            .padding(Dp(24.0))
            .background(col(20, 20, 28))
            .clip_rounded(Dp(12.0))
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "share-title", "Share Level"))
            .size(Sp(32.0))
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(st.level_name.clone())
            .size(Sp(16.0))
            .color(col(200, 200, 210)),
    )
    .child(
        RText(if st.level_verified {
            t(tr, "share-verified", "Verified")
        } else {
            t(tr, "share-unverified", "Beat the level to share it")
        })
        .size(Sp(14.0))
        .color(if st.level_verified {
            col(90, 200, 120)
        } else {
            col(230, 160, 70)
        }),
    )
    .child(spacer(16.0))
    .child(
        RText(t(tr, "share-export-title", "Export"))
            .size(Sp(18.0))
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        Column(
            Modifier::new()
                .fill_max_width()
                .padding(Dp(12.0))
                .background(col(12, 12, 18))
                .clip_rounded(Dp(8.0)),
        )
        .child(
            RText(if st.export_code.is_empty() {
                t(tr, "share-export-empty", "No code yet")
            } else {
                st.export_code.clone()
            })
            .size(Sp(13.0))
            .color(if st.export_code.is_empty() {
                col(180, 180, 190)
            } else {
                col(120, 200, 255)
            }),
        ),
    )
    .child(spacer(4.0))
    .child(
        RText(st.export_error.clone().unwrap_or_default())
            .size(Sp(13.0))
            .color(col(230, 110, 110)),
    )
    .child(spacer(8.0))
    .child(mk_button(
        &t(tr, "share-export", "Generate Code"),
        col(70, 110, 170),
        move || push(&a_export, UiAction::MakerExportCode),
    ))
    .child(mk_button(
        &t(tr, "share-copy", "Copy Code"),
        col(60, 140, 90),
        move || push(&a_copy, UiAction::MakerCopyCode),
    ))
    .child(spacer(20.0))
    .child(
        RText(t(tr, "share-import-title", "Import"))
            .size(Sp(18.0))
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        Column(
            Modifier::new()
                .fill_max_width()
                .padding(Dp(10.0))
                .background(col(45, 45, 60))
                .clip_rounded(Dp(8.0)),
        )
        .child(field),
    )
    .child(spacer(8.0))
    .child(mk_button(
        &t(tr, "share-import", "Import"),
        col(160, 120, 60),
        move || {
            push(
                &a_import,
                UiAction::MakerImportCode(import_state.borrow().text.clone()),
            )
        },
    ))
    .child(spacer(8.0))
    .child(Column(Modifier::new().align_items(AlignItems::CENTER)).children(tail));

    modal_shell(inner)
}

pub(crate) fn level_info_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_close = actions.clone();
    let a_save = actions.clone();

    let focus0: Rc<Cell<bool>> = remember_with_key("li_focus_0", || Cell::new(false));
    let focus1: Rc<Cell<bool>> = remember_with_key("li_focus_1", || Cell::new(false));
    let focus2: Rc<Cell<bool>> = remember_with_key("li_focus_2", || Cell::new(false));
    if matches!(st.overlay, OverlayMenu::LevelInfo) {
        push(
            &actions,
            UiAction::SetKeyboardCaptured(focus0.get() || focus1.get() || focus2.get()),
        );
    }

    let text_field = |idx: u8,
                      label: &'static str,
                      value: &str,
                      multiline: bool,
                      focus: Rc<Cell<bool>>|
     -> View {
        let state: Rc<RefCell<TextFieldState>> =
            remember_with_key(format!("li_state_{idx}"), || {
                RefCell::new(TextFieldState::new())
            });
        if !focus.get() && state.borrow().text != value {
            state.borrow_mut().text = value.to_string();
        }
        let on_change = {
            let a = actions.clone();
            Rc::new(move |v: String| push(&a, UiAction::LevelInfoSetText(idx, v)))
                as Rc<dyn Fn(String)>
        };
        let field = BasicTextField(
            state,
            Modifier::new().fill_max_width(),
            label,
            TextFieldConfig {
                line_limits: if multiline {
                    TextFieldLineLimits::MultiLine {
                        min_height_in_lines: 2,
                        max_height_in_lines: 6,
                    }
                } else {
                    TextFieldLineLimits::SingleLine
                },
                on_change: Some(on_change),
                focus_tracker: Some(focus),
                ..Default::default()
            },
        );
        Column(Modifier::new().fill_max_width().gap(Dp(4.0)))
            .child(RText(label).size(Sp(11.0)).color(col(150, 150, 170)))
            .child(
                Column(
                    Modifier::new()
                        .fill_max_width()
                        .padding(Dp(8.0))
                        .background(col(45, 45, 60))
                        .clip_rounded(Dp(8.0)),
                )
                .child(field),
            )
    };

    let mut tag_views: Vec<View> = Vec::new();
    for tag in LevelTag::ALL {
        let a = actions.clone();
        let included = st.info_tags.contains(&tag);
        let label = RText(tag.label()).size(Sp(12.0)).color(if included {
            RColor::WHITE
        } else {
            col(150, 150, 170)
        });
        tag_views.push(mk_chip(label, included, tag_color(tag), move || {
            push(&a, UiAction::LevelInfoToggleTag(tag))
        }));
    }
    let tag_row = FlowRow(
        Modifier::new().fill_max_width().gap(Dp(6.0)),
        FlowRowConfig::default(),
    )
    .child(tag_views);

    let mut preset_views: Vec<View> = Vec::new();
    for p in BoundaryPreset::ALL {
        let a = actions.clone();
        let selected = st.info_preset == Some(p);
        let label = RText(p.label()).size(Sp(11.0)).color(RColor::WHITE);
        preset_views.push(mk_chip(label, selected, col(90, 90, 120), move || {
            push(&a, UiAction::LevelInfoPreset(p))
        }));
    }
    let preset_row = FlowRow(
        Modifier::new().fill_max_width().gap(Dp(6.0)),
        FlowRowConfig::default(),
    )
    .child(preset_views);

    let a_wup = actions.clone();
    let a_wdn = actions.clone();
    let water_label = RText(match st.info_water {
        Some(level) => format!("  Water y = {level}  "),
        None => "  No water  ".to_string(),
    })
    .size(Sp(13.0))
    .color(col(200, 200, 210));
    let water_minus = mk_button_sm("-", move || push(&a_wdn, UiAction::LevelInfoWaterDelta(-1)));
    let water_plus = mk_button_sm("+", move || push(&a_wup, UiAction::LevelInfoWaterDelta(1)));
    let settings_row = Row(Modifier::new().gap(Dp(8.0)).align_items(AlignItems::CENTER))
        .child(water_minus)
        .child(water_label)
        .child(water_plus);

    let a_sup = actions.clone();
    let a_sdn = actions.clone();
    let a_sauto = actions.clone();
    let size_label = RText(if st.info_size_auto {
        "  Size: auto  ".to_string()
    } else {
        format!(
            "  Size {}×{}×{}  ",
            st.info_size[0], st.info_size[1], st.info_size[2]
        )
    })
    .size(Sp(13.0))
    .color(col(200, 200, 210));
    let size_minus = mk_button_sm("-", move || push(&a_sdn, UiAction::LevelInfoSizeDelta(-1)));
    let size_plus = mk_button_sm("+", move || push(&a_sup, UiAction::LevelInfoSizeDelta(1)));
    let size_auto_btn = mk_button_sm("Auto", move || push(&a_sauto, UiAction::LevelInfoSizeAuto));
    let size_row = Row(Modifier::new().gap(Dp(8.0)).align_items(AlignItems::CENTER))
        .child(size_minus)
        .child(size_label)
        .child(size_plus)
        .child(size_auto_btn);

    let a_hup = actions.clone();
    let a_hdn = actions.clone();
    let a_hauto = actions.clone();
    let height_label = RText(if st.info_height == 0 {
        "  Height: auto  ".to_string()
    } else {
        format!("  Height: {} cells  ", st.info_height)
    })
    .size(Sp(13.0))
    .color(col(200, 200, 210));
    let height_minus = mk_button_sm("-", move || {
        push(&a_hdn, UiAction::LevelInfoHeightDelta(-1))
    });
    let height_plus = mk_button_sm("+", move || push(&a_hup, UiAction::LevelInfoHeightDelta(1)));
    let height_auto_btn = mk_button_sm("Auto", move || {
        push(&a_hauto, UiAction::LevelInfoHeightAuto)
    });
    let height_row = Row(Modifier::new().gap(Dp(8.0)).align_items(AlignItems::CENTER))
        .child(height_minus)
        .child(height_label)
        .child(height_plus)
        .child(height_auto_btn);

    let cond_label = match st.info_clear_condition {
        ClearCondition::ReachGoal => "Reach Goal".to_string(),
        ClearCondition::CollectAllGlimmers => "Collect All Glimmers".to_string(),
        ClearCondition::DefeatAllProwlers => "Defeat All Prowlers".to_string(),
        ClearCondition::NoDeath => "No Death".to_string(),
        ClearCondition::TimeLimitMs(ms) => {
            format!("Time Limit · {}:{:02}", ms / 60_000, (ms / 1_000) % 60)
        }
    };
    let a_cond = actions.clone();
    let condition_row =
        Row(Modifier::new().gap(Dp(8.0)).align_items(AlignItems::CENTER)).child(mk_pill_button(
            RText(format!("Condition: {cond_label}"))
                .size(Sp(12.0))
                .color(RColor::WHITE),
            move || push(&a_cond, UiAction::LevelInfoCycleClearCondition),
        ));

    let limit_label = match st.info_clear_condition {
        ClearCondition::TimeLimitMs(ms) => {
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
    let time_row = Row(Modifier::new().gap(Dp(8.0)).align_items(AlignItems::CENTER))
        .child(mk_button_sm("-", move || {
            push(&a_tdn, UiAction::LevelInfoTimeLimitDelta(-15))
        }))
        .child(RText(limit_label).size(Sp(12.0)).color(col(200, 200, 210)))
        .child(mk_button_sm("+", move || {
            push(&a_tup, UiAction::LevelInfoTimeLimitDelta(15))
        }));

    let stats = format!(
        "Blocks: {}   ·   Entities: {}",
        st.info_blocks, st.info_entities
    );

    let body = Column(
        Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER),
    )
    .child(text_field(0, "Name", &st.info_name, false, focus0.clone()))
    .child(spacer(8.0))
    .child(text_field(
        1,
        "Author",
        &st.info_author,
        false,
        focus1.clone(),
    ))
    .child(spacer(8.0))
    .child(text_field(
        2,
        "Description",
        &st.info_description,
        true,
        focus2.clone(),
    ))
    .child(spacer(16.0))
    .child(RText("Tags").size(Sp(14.0)).color(col(180, 180, 190)))
    .child(spacer(6.0))
    .child(tag_row)
    .child(spacer(16.0))
    .child(
        RText("Level Settings")
            .size(Sp(14.0))
            .color(col(180, 180, 190)),
    )
    .child(spacer(6.0))
    .child(
        RText("Boundary presets: floor caught, walls rim, ceiling cap.")
            .size(Sp(11.0))
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(preset_row)
    .child(spacer(8.0))
    .child(settings_row)
    .child(spacer(8.0))
    .child(
        RText("Cycle the level’s win rule. Time limit uses ±15s below.")
            .size(Sp(11.0))
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(condition_row)
    .child(spacer(8.0))
    .child(time_row)
    .child(spacer(12.0))
    .child(
        RText("Type size: shrink from auto box, grow to enlarge.")
            .size(Sp(11.0))
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(size_row)
    .child(spacer(8.0))
    .child(
        RText("Height is the room/wall top; the ceiling sits there.")
            .size(Sp(11.0))
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(height_row)
    .child(spacer(12.0))
    .child(RText(stats).size(Sp(13.0)).color(col(200, 200, 210)))
    .child(spacer(12.0))
    .child(
        RText("Saving metadata does not reset verification.")
            .size(Sp(11.0))
            .color(col(130, 130, 150)),
    );

    let scroll_state = remember_scroll_state("level_info");
    let scroll = ScrollArea(
        Modifier::new().fill_max_width().max_height(Dp(460.0)),
        scroll_state,
        body,
    );

    let inner = Column(
        Modifier::new()
            .width(Dp(480.0))
            .max_height(Dp(720.0))
            .padding(Dp(24.0))
            .background(col(20, 20, 28))
            .clip_rounded(Dp(12.0))
            .align_items(AlignItems::CENTER),
    )
    .child(RText("Level Info").size(Sp(32.0)).color(RColor::WHITE))
    .child(spacer(6.0))
    .child(
        RText("Click a field to edit · tags show up in Browse")
            .size(Sp(12.0))
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
    .child(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push(&a_close, UiAction::LevelInfoClose),
    ));

    modal_shell(inner)
}

pub(crate) fn level_select_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
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
                    .size(Sp(12.0))
                    .color(col(220, 210, 120)),
            }
        } else {
            RText(t(tr, "uncleared", "Uncleared"))
                .size(Sp(12.0))
                .color(col(120, 125, 140))
        };
        Column(Modifier::new().align_items(AlignItems::CENTER)).child((
            mk_button(
                &format!("{}. {}", i + 1, title),
                col(70, 90, 120),
                move || push(&a, UiAction::PlayBundledLevel(i)),
            ),
            RText(teaches).size(Sp(12.0)).color(col(160, 165, 180)),
            status,
            spacer(4.0),
        ))
    };

    let mut inner = Column(
        Modifier::new()
            .width(Dp(420.0))
            .padding(Dp(24.0))
            .background(col(20, 20, 28))
            .clip_rounded(Dp(12.0))
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "level-select-title", "Tutorial Levels"))
            .size(Sp(32.0))
            .color(RColor::WHITE),
        spacer(12.0),
    ));

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

pub(crate) fn level_clear_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_edit = actions.clone();
    let a_retry = actions.clone();
    let a_remix = actions.clone();
    let a_menu = actions.clone();

    let mut body: Vec<View> = vec![
        RText(t(tr, "maker-clear-title", "Level Clear!"))
            .size(Sp(38.0))
            .color(RColor::WHITE),
        spacer(12.0),
        RText(if st.level_verified {
            t(tr, "maker-clear-verified", "Level Verified!")
        } else {
            String::new()
        })
        .size(Sp(20.0))
        .color(col(90, 200, 120)),
        spacer(6.0),
        RText(format!(
            "{}: {:.2}s",
            t(tr, "maker-time", "Time"),
            st.clear_time_secs
        ))
        .size(Sp(18.0))
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
        .size(Sp(18.0))
        .color(RColor::WHITE),
    );
    body.push(
        RText(format!(
            "{}: {}/{}",
            t(tr, "maker-glimmers-count", "Glimmers"),
            st.glimmers_collected,
            st.glimmers_total
        ))
        .size(Sp(18.0))
        .color(col(255, 220, 100)),
    );
    body.push(
        RText(format!(
            "{}: {}",
            t(tr, "maker-blocks-count", "Blocks"),
            st.blocks_placed
        ))
        .size(Sp(18.0))
        .color(RColor::WHITE),
    );

    body.push(spacer(16.0));
    if st.is_bundled {
        body.push(mk_button(
            &t(tr, "maker-remix", "Remix This Level"),
            col(150, 100, 220),
            move || push(&a_remix, UiAction::MakerRemix),
        ));
    } else {
        body.push(mk_button(
            &t(tr, "maker-btn-edit", "Edit Level"),
            col(70, 110, 170),
            move || push(&a_edit, UiAction::MakerDismissClear),
        ));
    }
    body.push(mk_button(
        &t(tr, "maker-retry", "Retry"),
        col(60, 140, 90),
        move || push(&a_retry, UiAction::MakerRetry),
    ));
    body.push(mk_button(
        &t(tr, "back-to-menu", "Back to Menu"),
        col(180, 60, 60),
        move || push(&a_menu, UiAction::QuitToTitle),
    ));

    let inner = Column(
        Modifier::new()
            .width(Dp(380.0))
            .padding(Dp(24.0))
            .background(col(20, 20, 28))
            .clip_rounded(Dp(12.0))
            .align_items(AlignItems::CENTER),
    )
    .children(body);

    modal_shell(inner)
}
