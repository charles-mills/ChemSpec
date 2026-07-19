//! Small, vendored SVG icon surface for the desktop UI.

use iced::Color;
use iced::widget::{Svg, svg};

const CHIP: &[u8] = include_bytes!("../assets/icons/chip-line.svg");
const CODE_BLOCK: &[u8] = include_bytes!("../assets/icons/majesticons/code-block-line.svg");
const KEY: &[u8] = include_bytes!("../assets/icons/majesticons/key-line.svg");
const ALERT_CIRCLE: &[u8] = include_bytes!("../assets/icons/majesticons/alert-circle-line.svg");
const ARROW_RIGHT: &[u8] = include_bytes!("../assets/icons/majesticons/arrow-right-line.svg");
const ATOM: &[u8] = include_bytes!("../assets/icons/majesticons/atom-2-line.svg");
const HELP: &[u8] = include_bytes!("../assets/icons/help-circle-line.svg");
const PENCIL: &[u8] = include_bytes!("../assets/icons/pencil-line.svg");
const SETTINGS: &[u8] = include_bytes!("../assets/icons/settings-line.svg");
const DICE_FACES: [&[u8]; 3] = [
    include_bytes!("../assets/icons/dice-5-line.svg"),
    include_bytes!("../assets/icons/dice-2-line.svg"),
    include_bytes!("../assets/icons/dice-3-line.svg"),
];

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

pub fn atom(size: f32, color: Color) -> Svg<'static> {
    icon(ATOM, size, color)
}

pub fn help(size: f32, color: Color) -> Svg<'static> {
    icon(HELP, size, color)
}

pub fn pencil(size: f32, color: Color) -> Svg<'static> {
    icon(PENCIL, size, color)
}

pub fn settings(size: f32, color: Color) -> Svg<'static> {
    icon(SETTINGS, size, color)
}

/// A die resting on `face` 0; higher faces let the dice-roll button tumble
/// through pip layouts while a roll spins.
pub fn dice(face: usize, size: f32, color: Color) -> Svg<'static> {
    icon(DICE_FACES[face % DICE_FACES.len()], size, color)
}
