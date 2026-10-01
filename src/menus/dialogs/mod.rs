use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{AlignItems, Color as RColor, Dp, Modifier, Sp};
use repose_ui::{Column, Text as RText, TextStyle};

use crate::menus::MenuState;
use crate::menus::action::UiAction;
use crate::menus::components::{mk_button, modal_shell, push, spacer};
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
