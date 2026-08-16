use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{AlignItems, AlignSelf, JustifyContent, Modifier};
use repose_material::Icon;
use repose_material::material3::{
    ButtonConfig, CardConfig, FilledTonalButton, IconButton, IconButtonColors, IconButtonConfig,
    Scaffold, ScaffoldConfig, TopAppBar, TopAppBarColors, TopAppBarConfig,
};
use repose_ui::{Column, Row, Text as RText, TextStyle, ViewExt};

use crate::app::SharedUi;
use crate::menus::action::UiAction;
use crate::menus::components::{Symbols, clickable_outlined_card, push, spacer};
use crate::menus::style::{radius, sp, t, tok};

pub fn splash_ui() -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(tok::bg_deep()),
    )
    .child(RText("RUSTBOX").size(52.0).color(tok::text()))
    .child(spacer(8.0))
    .child(
        RText("Build. Play. Share.")
            .size(16.0)
            .color(tok::text_dim()),
    )
}

pub fn loading_ui(st: &SharedUi) -> View {
    let pct = st.loading_progress.clamp(0.0, 1.0);
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(tok::bg_deep()),
    )
    .child(RText("Loading worlds…").size(28.0).color(tok::text()))
    .child(spacer(16.0))
    .child(
        RText(format!("{:.0}%", pct * 100.0))
            .size(16.0)
            .color(tok::text_dim()),
    )
    .child(spacer(12.0))
    .child(
        Column(
            Modifier::new()
                .width(320.0)
                .height(10.0)
                .background(tok::bg_elevated())
                .clip_rounded(6.0),
        )
        .child(Column(
            Modifier::new()
                .width((320.0 * pct).max(1.0))
                .height(10.0)
                .background(tok::accent())
                .clip_rounded(6.0)
                .align_self(AlignSelf::FLEX_START),
        )),
    )
}

/// Creator-first home - an M3 workbench, not a template launcher.
pub fn title_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_settings = actions.clone();
    let a_credits = actions.clone();

    Scaffold(
        move |_insets| {
            let tr = &st.translations;
            let a_new = actions.clone();
            let a_campaign = actions.clone();
            let a_browse = actions.clone();
            let a_online = actions.clone();
            let a_quit = actions.clone();

            let mut hero: Vec<View> = Vec::new();
            hero.push(
                RText(t(tr, "app-title", "Rustbox"))
                    .size(48.0)
                    .color(tok::text()),
            );
            hero.push(spacer(6.0));
            hero.push(
                RText("3D creator toolkit - place blocks, wire logic, ship levels.")
                    .size(15.0)
                    .color(tok::text_dim()),
            );
            hero.push(spacer(28.0));
            hero.push(
                Row(Modifier::new().gap(sp::MD).fill_max_width()).children(vec![
                    half_card("New World", Symbols::ADD, move || {
                        push(&a_new, UiAction::StartGame)
                    }),
                    half_card("My Worlds", Symbols::FOLDER_OPEN, move || {
                        push(&a_browse, UiAction::BrowseOpen)
                    }),
                    half_card("Community", Symbols::CLOUD, move || {
                        push(&a_online, UiAction::OnlineOpen)
                    }),
                ]),
            );
            hero.push(spacer(sp::MD));
            hero.push(action_card(
                "LEARN",
                &t(tr, "play-levels", "Campaign"),
                "Levels (very small) to get a feel for the the builder",
                Symbols::FLAG,
                tok::play(),
                move || push(&a_campaign, UiAction::OpenLevelSelect),
            ));
            hero.push(Column(Modifier::new().flex_grow(1.0)));
            hero.push(FilledTonalButton(
                Modifier::new().min_height(36.0).padding(10.0),
                move || push(&a_quit, UiAction::QuitApp),
                ButtonConfig::default(),
                || RText("Quit").size(14.0).color(tok::text_mute()),
            ));

            Column(
                Modifier::new()
                    .fill_max_size()
                    .padding(sp::XL)
                    .gap(sp::SM)
                    .align_items(AlignItems::CENTER)
                    .justify_content(JustifyContent::CENTER),
            )
            .children(hero)
        },
        ScaffoldConfig {
            top_bar: Some(TopAppBar(
                RText("RUSTBOX").size(20.0).color(tok::text()),
                None,
                None,
                vec![
                    IconButton(
                        Icon(Symbols::SETTINGS).size(22.0),
                        move || push(&a_settings, UiAction::OpenSettings),
                        IconButtonConfig {
                            container_size: Some(40.0),
                            colors: IconButtonColors {
                                container_color: tok::bg_elevated(),
                                content_color: tok::text(),
                                disabled_container_color: tok::bg_panel_solid(),
                                disabled_content_color: tok::text_mute(),
                            },
                            ..Default::default()
                        },
                    ),
                    IconButton(
                        Icon(Symbols::INFO).size(22.0),
                        move || push(&a_credits, UiAction::OpenCredits),
                        IconButtonConfig {
                            container_size: Some(40.0),
                            colors: IconButtonColors {
                                container_color: tok::bg_elevated(),
                                content_color: tok::text(),
                                disabled_container_color: tok::bg_panel_solid(),
                                disabled_content_color: tok::text_mute(),
                            },
                            ..Default::default()
                        },
                    ),
                ],
                TopAppBarConfig {
                    colors: TopAppBarColors {
                        container_color: tok::bg_rail(),
                        title_content_color: tok::text(),
                        action_icon_content_color: tok::text(),
                        navigation_icon_content_color: tok::text(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )),
            container_color: tok::bg_deep(),
            ..Default::default()
        },
    )
}

fn action_card(
    kicker: &str,
    title: &str,
    subtitle: &str,
    icon: repose_material::Symbol,
    accent: repose_core::prelude::Color,
    on_click: impl Fn() + 'static,
) -> View {
    let kicker = kicker.to_string();
    let title = title.to_string();
    let subtitle = subtitle.to_string();
    clickable_outlined_card(
        on_click,
        Modifier::new()
            .fill_max_width()
            .padding(16.0)
            .background(tok::bg_panel_solid())
            .clip_rounded(radius::LG),
        CardConfig::default(),
        move || {
            Row(Modifier::new()
                .fill_max_width()
                .gap(14.0)
                .align_items(AlignItems::CENTER))
            .child((
                Column(
                    Modifier::new()
                        .width(44.0)
                        .height(44.0)
                        .background(accent)
                        .clip_rounded(radius::MD)
                        .justify_content(JustifyContent::CENTER)
                        .align_items(AlignItems::CENTER),
                )
                .child(Icon(icon).size(22.0).color(tok::text())),
                Column(Modifier::new().flex_grow(1.0)).child((
                    RText(kicker.clone()).size(11.0).color(tok::text_mute()),
                    RText(title.clone()).size(20.0).color(tok::text()),
                    RText(subtitle.clone()).size(13.0).color(tok::text_dim()),
                )),
            ))
        },
    )
}

fn half_card(title: &str, icon: repose_material::Symbol, on_click: impl Fn() + 'static) -> View {
    let title = title.to_string();
    clickable_outlined_card(
        on_click,
        Modifier::new()
            .flex_grow(1.0)
            .padding(14.0)
            .background(tok::bg_elevated())
            .clip_rounded(radius::MD),
        CardConfig::default(),
        move || {
            Column(Modifier::new().gap(8.0).align_items(AlignItems::FLEX_START)).child((
                Icon(icon).size(22.0).color(tok::accent()),
                RText(title.clone()).size(15.0).color(tok::text()),
            ))
        },
    )
}
