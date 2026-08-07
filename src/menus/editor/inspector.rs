use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{AlignItems, Modifier};
use repose_ui::{Column, Row, Text as RText, TextStyle};

use crate::app::SharedUi;
use crate::maker::entity_data::{ContainedItem, EntityKind};
use crate::maker::track::TrackMode;
use crate::menus::action::UiAction;
use crate::menus::components::{
    Symbols, inspector_section, mk_icon_button, mk_pill_button, push_ui, spacer,
};
use crate::menus::style::{t, tok};

fn stepper_row(
    label: String,
    value: String,
    on_minus: impl Fn() + 'static,
    on_plus: impl Fn() + 'static,
) -> View {
    Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER)
        .gap(6.0))
    .children(vec![
        mk_icon_button(Symbols::REMOVE, true, on_minus),
        RText(format!("{label}: {value}"))
            .size(12.0)
            .color(tok::text()),
        mk_icon_button(Symbols::ADD, true, on_plus),
    ])
}

pub fn inspector_panel(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let mut body: Vec<View> = vec![
        RText(t(tr, "inspector-title", "Inspector"))
            .size(12.0)
            .color(tok::text_dim()),
        spacer(6.0),
    ];

    if let Some(e) = &st.selected_entity_data {
        let (label_key, label_fb) = match e.kind {
            EntityKind::LaunchPad => ("maker-ent-pad", "Launch Pad"),
            EntityKind::Seal => ("maker-ent-seal", "Seal"),
            EntityKind::DriftPlate => ("maker-ent-drift", "Drift Plate"),
            EntityKind::Prowler => ("toolbar-prowler", "Prowler"),
            EntityKind::Glimmer => ("maker-ent-glimmer", "Glimmer"),
            EntityKind::TriggerOrb => ("toolbar-trigger", "Trigger Orb"),
            EntityKind::RelayGate => ("toolbar-gate", "Relay Gate"),
            EntityKind::Checkpoint => ("toolbar-checkpoint", "Checkpoint"),
            EntityKind::Teleporter => ("toolbar-teleport", "Teleporter"),
            EntityKind::Fan => ("toolbar-fan", "Fan"),
            EntityKind::Bumper => ("toolbar-bumper", "Bumper"),
            EntityKind::Crate => ("toolbar-crate", "Crate"),
            EntityKind::Key => ("toolbar-key", "Key"),
            EntityKind::LockGate => ("toolbar-lock", "Lock Gate"),
            EntityKind::HealOrb => ("toolbar-heal", "Heal Orb"),
            EntityKind::SpeedRing => ("toolbar-speed", "Speed Ring"),
            EntityKind::CrumblePlate => ("toolbar-crumble", "Crumble Plate"),
            EntityKind::Cannon => ("toolbar-cannon", "Cannon"),
            EntityKind::OnOffSwitch => ("toolbar-onoff", "On/Off Switch"),
            EntityKind::TossCrate => ("toolbar-tosscrate", "Toss Crate"),
            EntityKind::Sign => ("toolbar-sign", "Sign"),
        };

        body.push(inspector_section(
            "Selection",
            vec![
                RText(t(tr, label_key, label_fb))
                    .size(16.0)
                    .color(tok::text()),
                RText(format!(
                    "{} ({},{},{})",
                    t(tr, "inspector-cell", "Cell"),
                    e.cell[0],
                    e.cell[1],
                    e.cell[2]
                ))
                .size(12.0)
                .color(tok::text_dim()),
            ],
        ));

        if !matches!(
            e.kind,
            EntityKind::Checkpoint | EntityKind::Key | EntityKind::Sign
        ) {
            let (param_key, param_fb, step) = match e.kind {
                EntityKind::Glimmer => ("inspector-value", "Value", 0.5),
                EntityKind::LaunchPad => ("inspector-impulse", "Impulse", 0.5),
                EntityKind::Seal => ("inspector-glimmers", "Glimmers", 1.0),
                EntityKind::DriftPlate => ("inspector-period", "Period", 0.5),
                EntityKind::Prowler => ("inspector-speed", "Speed", 0.5),
                EntityKind::TriggerOrb => ("inspector-cooldown", "Cooldown", 0.5),
                EntityKind::RelayGate => ("inspector-duration", "Duration", 0.5),
                EntityKind::Teleporter => ("inspector-cooldown", "Cooldown", 0.1),
                EntityKind::Fan => ("inspector-strength", "Strength", 1.0),
                EntityKind::Bumper => ("inspector-strength", "Strength", 1.0),
                EntityKind::Crate => ("inspector-breakable", "Breakable", 1.0),
                EntityKind::HealOrb => ("inspector-armor", "Armor", 1.0),
                EntityKind::SpeedRing => ("inspector-duration", "Duration", 0.25),
                EntityKind::CrumblePlate => ("inspector-delay", "Delay", 0.05),
                EntityKind::LockGate => ("inspector-open-for", "Open For", 0.5),
                EntityKind::Cannon => ("inspector-arc", "Arc", 1.0),
                EntityKind::OnOffSwitch => ("inspector-starts-on", "Starts On", 1.0),
                EntityKind::TossCrate => ("inspector-breakable", "Breakable", 1.0),
                EntityKind::Checkpoint | EntityKind::Key | EntityKind::Sign => unreachable!(),
            };
            let a_minus = actions.clone();
            let a_plus = actions.clone();
            body.push(inspector_section(
                "Behavior",
                vec![stepper_row(
                    t(tr, param_key, param_fb),
                    format!("{:.1}", e.param),
                    move || push_ui(&a_minus, UiAction::MakerInspParamDelta(-step)),
                    move || push_ui(&a_plus, UiAction::MakerInspParamDelta(step)),
                )],
            ));
        }

        if e.kind == EntityKind::Sign {
            let a_edit = actions.clone();
            body.push(inspector_section(
                "Behavior",
                vec![
                    Row(Modifier::new()
                        .fill_max_width()
                        .align_items(AlignItems::CENTER)
                        .gap(6.0))
                    .children(vec![
                        RText(t(tr, "inspector-sign-text", "Sign Text"))
                            .size(12.0)
                            .color(tok::text()),
                        mk_pill_button(
                            RText(t(tr, "inspector-edit-text", "Edit Text"))
                                .size(12.0)
                                .color(tok::text()),
                            move || push_ui(&a_edit, UiAction::MakerInspEditSignText),
                        ),
                    ]),
                ],
            ));
        }

        if e.kind.supports_contents() {
            let a_cycle = actions.clone();
            let a_cycle2 = actions.clone();
            body.push(inspector_section(
                "Behavior",
                vec![stepper_row(
                    t(tr, "inspector-contents", "Contains"),
                    e.contents.label(),
                    move || push_ui(&a_cycle, UiAction::MakerInspCycleContents),
                    move || push_ui(&a_cycle2, UiAction::MakerInspCycleContents),
                )],
            ));

            if matches!(e.contents, ContainedItem::Glimmers(_)) {
                let a_minus = actions.clone();
                let a_plus = actions.clone();
                body.push(inspector_section(
                    "Behavior",
                    vec![stepper_row(
                        t(tr, "inspector-count", "Count"),
                        e.contents.label(),
                        move || push_ui(&a_minus, UiAction::MakerInspContentsDelta(-1)),
                        move || push_ui(&a_plus, UiAction::MakerInspContentsDelta(1)),
                    )],
                ));
            }
        }

        let needs_link_for_contents =
            matches!(e.contents, ContainedItem::Key) && e.kind.supports_contents();

        if e.kind.uses_link() || needs_link_for_contents {
            let a_minus = actions.clone();
            let a_plus = actions.clone();
            body.push(inspector_section(
                "Linking",
                vec![stepper_row(
                    t(tr, "inspector-channel", "Channel"),
                    format!("{}", e.link),
                    move || push_ui(&a_minus, UiAction::MakerInspLinkDelta(-1)),
                    move || push_ui(&a_plus, UiAction::MakerInspLinkDelta(1)),
                )],
            ));
        }

        if matches!(
            e.kind,
            EntityKind::LaunchPad | EntityKind::Prowler | EntityKind::Fan | EntityKind::Teleporter
        ) {
            let a_minus = actions.clone();
            let a_plus = actions.clone();
            body.push(inspector_section(
                "Transform",
                vec![stepper_row(
                    t(tr, "inspector-yaw", "Yaw"),
                    format!("{}°", e.yaw_deg as i32),
                    move || push_ui(&a_minus, UiAction::MakerInspYawDelta(-45.0)),
                    move || push_ui(&a_plus, UiAction::MakerInspYawDelta(45.0)),
                )],
            ));
        }

        if matches!(e.kind, EntityKind::DriftPlate | EntityKind::Prowler) {
            let cur = e
                .track
                .map(|id| format!("#{id}"))
                .unwrap_or_else(|| t(tr, "inspector-none", "None"));
            let a_cycle = actions.clone();
            body.push(inspector_section(
                "Track",
                vec![mk_pill_button(
                    RText(format!("{}: {}", t(tr, "inspector-track", "Track"), cur)),
                    move || push_ui(&a_cycle, UiAction::MakerInspTrackCycle),
                )],
            ));
        }

        let a_del = actions.clone();
        body.push(inspector_section(
            "Delete",
            vec![mk_pill_button(
                RText(t(tr, "inspector-delete", "Delete")),
                move || push_ui(&a_del, UiAction::MakerInspDeleteEntity),
            )],
        ));
    } else if let Some(track) = &st.active_track_data {
        let mode_label = match track.mode {
            TrackMode::PingPong => t(tr, "inspector-mode-pingpong", "PingPong"),
            TrackMode::Loop => t(tr, "inspector-mode-loop", "Loop"),
        };

        body.push(inspector_section(
            "Track",
            vec![
                RText(format!(
                    "{} #{}",
                    t(tr, "toolbar-tracks", "Tracks"),
                    track.id
                ))
                .size(16.0)
                .color(tok::text()),
                RText(format!(
                    "{}: {}",
                    t(tr, "inspector-points", "Points"),
                    track.points.len()
                ))
                .size(12.0)
                .color(tok::text_dim()),
            ],
        ));

        let a_mode = actions.clone();
        body.push(inspector_section(
            "Mode",
            vec![mk_pill_button(
                RText(format!(
                    "{}: {}",
                    t(tr, "inspector-mode", "Mode"),
                    mode_label
                )),
                move || push_ui(&a_mode, UiAction::MakerInspTrackModeToggle),
            )],
        ));

        let a_minus = actions.clone();
        let a_plus = actions.clone();
        body.push(inspector_section(
            "Speed",
            vec![stepper_row(
                t(tr, "inspector-speed", "Speed"),
                format!("{:.1}", track.speed),
                move || push_ui(&a_minus, UiAction::MakerInspTrackSpeedDelta(-0.5)),
                move || push_ui(&a_plus, UiAction::MakerInspTrackSpeedDelta(0.5)),
            )],
        ));

        let a_rev = actions.clone();
        body.push(inspector_section(
            "Mode",
            vec![mk_pill_button(
                RText(t(tr, "inspector-reverse", "Reverse")),
                move || push_ui(&a_rev, UiAction::MakerInspTrackReverse),
            )],
        ));

        let a_del = actions.clone();
        body.push(inspector_section(
            "Delete",
            vec![mk_pill_button(
                RText(t(tr, "inspector-delete", "Delete")),
                move || push_ui(&a_del, UiAction::MakerInspTrackDelete),
            )],
        ));
    } else {
        body.push(
            RText(t(tr, "inspector-hint", "Select an entity or track"))
                .size(13.0)
                .color(tok::text_dim()),
        );
    }

    let mirror_label = match st.mirror {
        1 => "X",
        2 => "Z",
        3 => "X+Z",
        _ => "Off",
    };
    body.push(spacer(8.0));
    body.push(
        RText(format!(
            "{}: {}",
            t(tr, "inspector-mirror", "Mirror"),
            mirror_label
        ))
        .size(12.0)
        .color(tok::text_dim()),
    );

    Column(
        Modifier::new()
            .width(196.0)
            .padding(12.0)
            .background(tok::bg_elevated())
            .clip_rounded(tok::R_PILL)
            .align_items(AlignItems::FLEX_START),
    )
    .children(body)
}
