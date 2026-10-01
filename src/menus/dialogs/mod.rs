use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use repose_core::prelude::{AlignItems, AlignSelf, Color as RColor, Dp, Modifier, Sp, remember};
use repose_core::{ImeAction, KeyboardOptions, KeyboardType, TextFieldLineLimits, View};
use repose_ui::{
    BasicTextField, Column, Text as RText, TextFieldConfig, TextFieldState, TextStyle, ViewExt,
};

use crate::menus::MenuState;
use crate::menus::action::UiAction;
use crate::menus::components::{mk_button, mk_pill_button, modal_shell, push, spacer};
use crate::menus::style::{col, t};

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
    let tr = &st.translations;

    modal_shell(pause_panel(tr, st, a1, a3, a_retry))
}

fn pause_panel(
    tr: &std::collections::HashMap<String, String>,
    st: &MenuState,
    a1: Arc<Mutex<Vec<UiAction>>>,
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
