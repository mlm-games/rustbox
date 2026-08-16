use repose_core::View;
use repose_core::prelude::{Color, Modifier, StateColors, theme};
use repose_material::material3::{Card, CardConfig};
use repose_material::ripple::{RippleConfig, ripple};
use repose_ui::{Column, ViewExt};

fn card_state_colors(bg: Color) -> StateColors {
    let th = theme();
    StateColors {
        default: Color::TRANSPARENT,
        hovered: Color::TRANSPARENT,
        focused: Color::TRANSPARENT,
        pressed: Color::TRANSPARENT,
        dragged: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
        disabled: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
    }
}

pub fn clickable_outlined_card(
    on_click: impl Fn() + 'static,
    modifier: Modifier,
    config: CardConfig,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let bg = th.surface;
    let m = modifier
        .state_colors(card_state_colors(bg))
        .indication(ripple(RippleConfig {
            color: Some(th.on_surface),
            bounded: true,
            ..Default::default()
        }))
        .clickable()
        .on_click(on_click);
    Card(
        CardConfig {
            modifier: m,
            container_color: bg,
            border: Some((1.0, th.outline_variant)),
            shape_radius: th.shapes.medium,
            ..config
        },
        || Column(Modifier::new().fill_max_size()).child(content()),
    )
}
