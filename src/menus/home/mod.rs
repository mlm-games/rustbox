use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{AlignItems, AlignSelf, Dp, JustifyContent, Modifier, Sp};
use repose_material::Icon;
use repose_material::material3::{ButtonConfig, CardConfig, FilledTonalButton};
use repose_ui::{Column, Row, Text as RText, TextStyle, ViewExt, ZStack};

use crate::menus::MenuState;
use crate::menus::action::UiAction;
use crate::menus::components::{Symbols, clickable_outlined_card, mk_icon_button, push, spacer};
use crate::menus::style::{radius, sp, t, tok};

pub fn splash_ui() -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(tok::bg_deep()),
    )
    .child(
        RText("Rustbox (pre-alpha)")
            .size(Sp(52.0))
            .color(tok::text()),
    )
    .child(spacer(8.0))
    .child(
        RText("Build. Play. Share.")
            .size(Sp(16.0))
            .color(tok::text_dim()),
    )
}

pub fn loading_ui(st: &MenuState) -> View {
    let pct = st.loading_progress.clamp(0.0, 1.0);
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(tok::bg_deep()),
    )
    .child(RText("Loading worlds…").size(Sp(28.0)).color(tok::text()))
    .child(spacer(16.0))
    .child(
        RText(format!("{:.0}%", pct * 100.0))
            .size(Sp(16.0))
            .color(tok::text_dim()),
    )
    .child(spacer(12.0))
    .child(
        Column(
            Modifier::new()
                .width(Dp(320.0))
                .height(Dp(10.0))
                .background(tok::bg_elevated())
                .clip_rounded(Dp(6.0)),
        )
        .child(Column(
            Modifier::new()
                .width(Dp((320.0 * pct).max(1.0)))
                .height(Dp(10.0))
                .background(tok::accent())
                .clip_rounded(Dp(6.0))
                .align_self(AlignSelf::FLEX_START),
        )),
    )
}

/// Creator-first home - an M3 workbench, not a template launcher.
pub fn title_ui(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_new = actions.clone();
    let a_browse = actions.clone();
    let a_campaign = actions.clone();
    let a_online = actions.clone();
    let a_quit = actions.clone();
    let a_settings = actions.clone();
    let a_credits = actions.clone();

    let mut hero: Vec<View> = Vec::new();
    hero.push(
        RText(t(tr, "app-title", "Rustbox"))
            .size(Sp(48.0))
            .color(tok::text()),
    );
    hero.push(spacer(6.0));
    hero.push(
        RText("3D creator toolkit - place blocks, wire logic, ship levels.")
            .size(Sp(15.0))
            .color(tok::text_dim()),
    );
    hero.push(spacer(28.0));
    hero.push(
        Row(Modifier::new().gap(Dp(sp::MD)).fill_max_width()).children(vec![
            half_card("New World", Symbols::ADD, move || {
                push(&a_new, UiAction::StartGame)
            }),
            half_card("My Worlds", Symbols::FOLDER_OPEN, move || {
                push(&a_browse, UiAction::BrowseOpen)
            }),
        ]),
    );
    hero.push(spacer(sp::MD));
    hero.push(
        Row(Modifier::new().gap(Dp(sp::MD)).fill_max_width()).children(vec![
            half_card("Campaign", Symbols::FLAG, move || {
                push(&a_campaign, UiAction::OpenLevelSelect)
            }),
            half_card("Community", Symbols::PUBLIC, move || {
                push(&a_online, UiAction::OnlineOpen)
            }),
        ]),
    );
    hero.push(spacer(sp::MD));
    hero.push(Column(Modifier::new().flex_grow(1.0)));
    hero.push(FilledTonalButton(
        Modifier::new().min_height(Dp(36.0)).padding(Dp(10.0)),
        move || push(&a_quit, UiAction::QuitApp),
        ButtonConfig::default(),
        || RText("Quit").size(Sp(14.0)).color(tok::text_mute()),
    ));

    ZStack(Modifier::new().fill_max_size().background(tok::bg_deep()))
        .child(
            Column(
                Modifier::new()
                    .fill_max_size()
                    .padding(Dp(sp::XL))
                    .gap(Dp(sp::SM))
                    .align_items(AlignItems::CENTER)
                    .justify_content(JustifyContent::CENTER),
            )
            .children(hero),
        )
        .child(
            Column(
                Modifier::new()
                    .fill_max_size()
                    .align_items(AlignItems::FLEX_END)
                    .padding(Dp(sp::MD)),
            )
            .child(Row(Modifier::new().gap(Dp(8.0))).children(vec![
                mk_icon_button(Symbols::SETTINGS, true, move || {
                    push(&a_settings, UiAction::OpenSettings)
                }),
                mk_icon_button(Symbols::INFO, true, move || {
                    push(&a_credits, UiAction::OpenCredits)
                }),
            ])),
        )
}

fn half_card(title: &str, icon: repose_material::Symbol, on_click: impl Fn() + 'static) -> View {
    let title = title.to_string();
    clickable_outlined_card(
        on_click,
        Modifier::new()
            .flex_grow(1.0)
            .padding(Dp(14.0))
            .background(tok::bg_elevated())
            .clip_rounded(Dp(radius::MD)),
        CardConfig::default(),
        move || {
            Column(
                Modifier::new()
                    .gap(Dp(8.0))
                    .align_items(AlignItems::FLEX_START),
            )
            .child((
                Icon(icon).size(Sp(22.0)).color(tok::accent()),
                RText(title.clone()).size(Sp(15.0)).color(tok::text()),
            ))
        },
    )
}
