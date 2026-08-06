use std::time::Duration;

use repose_core::View;
use repose_core::prelude::{AlignItems, AnimationSpec, Easing, JustifyContent, Modifier};
use repose_ui::anim_ext::{AnimatedVisibilityConfig, EnterTransition, ExitTransition};
use repose_ui::{Column, ViewExt};

use crate::menus::style::tok;

pub fn popup_anim_config(key: &str) -> AnimatedVisibilityConfig {
    AnimatedVisibilityConfig {
        key: key.into(),
        spec: AnimationSpec::tween(Duration::from_millis(200), Easing::EaseOut),
        enter: EnterTransition::ScaleIn { initial: 0.95 },
        exit: ExitTransition::ScaleOut { target: 0.95 },
    }
}

/// Full-screen dimmer + centered content. Prefer Material dialogs long-term.
pub fn modal_shell(inner: View) -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(tok::scrim())
            .clickable()
            .focusable(false),
    )
    .child(inner)
}

pub fn modal_card(width: f32, children: impl IntoIterator<Item = View>) -> View {
    Column(
        Modifier::new()
            .width(width)
            .padding(24.0)
            .background(tok::bg_modal())
            .clip_rounded(tok::R_MD)
            .align_items(AlignItems::CENTER),
    )
    .children(children.into_iter().collect::<Vec<View>>())
}
