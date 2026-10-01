use repose_core::View;
use repose_core::prelude::{AlignItems, Dp, Modifier, Sp};
use repose_ui::{Column, Text as RText, TextStyle};

use crate::menus::style::tok;

pub fn inspector_section(title: &str, body: Vec<View>) -> View {
    let mut kids: Vec<View> = vec![
        RText(title.to_string())
            .size(Sp(11.0))
            .color(tok::text_dim()),
        super::buttons::spacer(4.0),
    ];
    kids.extend(body);
    kids.push(super::buttons::spacer(8.0));
    Column(
        Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::FLEX_START)
            .gap(Dp(2.0)),
    )
    .children(kids)
}
