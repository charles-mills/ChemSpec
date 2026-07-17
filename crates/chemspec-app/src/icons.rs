//! Small, vendored SVG icon surface for the desktop UI.

use iced::Color;
use iced::widget::{Svg, svg};

const CHIP: &[u8] = include_bytes!("../assets/icons/chip-line.svg");
const CODE_BLOCK: &[u8] = include_bytes!("../assets/icons/majesticons/code-block-line.svg");
const KEY: &[u8] = include_bytes!("../assets/icons/majesticons/key-line.svg");
const ALERT_CIRCLE: &[u8] = include_bytes!("../assets/icons/majesticons/alert-circle-line.svg");
const ARROW_RIGHT: &[u8] = include_bytes!("../assets/icons/majesticons/arrow-right-line.svg");

fn icon(bytes: &'static [u8], size: f32, color: Color) -> Svg<'static> {
    svg(svg::Handle::from_memory(bytes))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
}

pub fn chip(size: f32, color: Color) -> Svg<'static> {
    icon(CHIP, size, color)
}

pub fn codex(size: f32, color: Color) -> Svg<'static> {
    icon(CODE_BLOCK, size, color)
}

pub fn api_key(size: f32, color: Color) -> Svg<'static> {
    icon(KEY, size, color)
}

pub fn alert(size: f32, color: Color) -> Svg<'static> {
    icon(ALERT_CIRCLE, size, color)
}

pub fn arrow_right(size: f32, color: Color) -> Svg<'static> {
    icon(ARROW_RIGHT, size, color)
}
