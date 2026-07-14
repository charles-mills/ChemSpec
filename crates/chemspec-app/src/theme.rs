//! `ChemSpec`'s visual design system.
//!
//! The palette and surface hierarchy translate the quiet, instrument-like
//! character of the `ChemistrySim` reference into reusable Iced styles. Layout
//! code should use these tokens instead of introducing one-off values.

use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector, border};

pub mod color {
    use iced::Color;

    pub const CANVAS: Color = Color::from_rgb(0.035, 0.043, 0.055);
    pub const CANVAS_RAISED: Color = Color::from_rgb(0.047, 0.059, 0.075);
    pub const PANEL: Color = Color::from_rgb(0.063, 0.078, 0.098);
    pub const SURFACE: Color = Color::from_rgb(0.082, 0.102, 0.125);
    pub const SURFACE_HOVER: Color = Color::from_rgb(0.105, 0.133, 0.165);
    pub const SURFACE_ACTIVE: Color = Color::from_rgb(0.125, 0.157, 0.192);

    pub const TEXT: Color = Color::from_rgb(0.957, 0.969, 0.980);
    pub const TEXT_SOFT: Color = Color::from_rgb(0.765, 0.808, 0.851);
    pub const MUTED: Color = Color::from_rgb(0.604, 0.651, 0.698);
    pub const FAINT: Color = Color::from_rgb(0.416, 0.463, 0.510);

    pub const LINE: Color = Color::from_rgb(0.165, 0.196, 0.235);
    pub const LINE_STRONG: Color = Color::from_rgb(0.231, 0.282, 0.337);

    pub const ACCENT: Color = Color::from_rgb(0.561, 0.773, 1.0);
    pub const ACCENT_HOVER: Color = Color::from_rgb(0.659, 0.824, 1.0);
    pub const ACCENT_SOFT: Color = Color::from_rgb(0.102, 0.180, 0.263);
    pub const ACCENT_FAINT: Color = Color::from_rgb(0.067, 0.122, 0.180);

    pub const SUCCESS: Color = Color::from_rgb(0.431, 0.839, 0.576);
    pub const SUCCESS_SOFT: Color = Color::from_rgb(0.071, 0.180, 0.118);
    pub const WARNING: Color = Color::from_rgb(0.945, 0.710, 0.353);
    pub const DANGER: Color = Color::from_rgb(0.984, 0.443, 0.506);
}

pub mod space {
    pub const XXS: f32 = 4.0;
    pub const XS: f32 = 8.0;
    pub const SM: f32 = 12.0;
    pub const MD: f32 = 16.0;
    pub const LG: f32 = 24.0;
    pub const XL: f32 = 32.0;
}

pub mod radius {
    pub const CONTROL: f32 = 8.0;
    pub const PANEL: f32 = 12.0;
    pub const FRAME: f32 = 16.0;
    pub const PILL: f32 = 999.0;
}

pub mod type_scale {
    pub const MICRO: f32 = 10.0;
    pub const CAPTION: f32 = 12.0;
    pub const BODY: f32 = 14.0;
    pub const BODY_LARGE: f32 = 16.0;
    pub const TITLE: f32 = 22.0;
    pub const DISPLAY: f32 = 30.0;
}

pub mod breakpoint {
    pub const MOBILE: f32 = 720.0;
    pub const DESKTOP: f32 = 1_120.0;
}

pub fn app_theme() -> Theme {
    Theme::custom(
        "ChemSpec Lab",
        iced::theme::Palette {
            background: color::CANVAS,
            text: color::TEXT,
            primary: color::ACCENT,
            success: color::SUCCESS,
            warning: color::WARNING,
            danger: color::DANGER,
        },
    )
}

fn border_style(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: border::Radius::new(radius),
    }
}

pub fn app_background(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::CANVAS)
        .color(color::TEXT)
}

pub fn chrome(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::CANVAS_RAISED)
        .border(border_style(color::LINE, 1.0, radius::PANEL))
}

pub fn frame(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::PANEL)
        .border(border_style(color::LINE, 1.0, radius::FRAME))
        .shadow(Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.32),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        })
}

pub fn panel(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::CANVAS_RAISED)
        .border(border_style(color::LINE, 1.0, radius::PANEL))
}

pub fn inset(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::CANVAS)
        .border(border_style(color::LINE_STRONG, 1.0, radius::CONTROL))
}

pub fn raised(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::SURFACE)
        .border(border_style(color::LINE, 1.0, radius::CONTROL))
}

pub fn accent_tint(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::ACCENT_FAINT)
        .border(border_style(color::ACCENT_SOFT, 1.0, radius::CONTROL))
}

pub fn success_tint(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::SUCCESS_SOFT)
        .border(border_style(
            Color::from_rgba(0.431, 0.839, 0.576, 0.42),
            1.0,
            radius::PILL,
        ))
}

pub fn primary_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, border_color, text_color, shadow) = match status {
        button::Status::Active => (
            color::ACCENT,
            color::ACCENT,
            color::CANVAS,
            Shadow {
                color: Color::from_rgba(0.176, 0.514, 0.855, 0.26),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 14.0,
            },
        ),
        button::Status::Hovered => (
            color::ACCENT_HOVER,
            color::ACCENT_HOVER,
            color::CANVAS,
            Shadow {
                color: Color::from_rgba(0.176, 0.514, 0.855, 0.38),
                offset: Vector::new(0.0, 6.0),
                blur_radius: 18.0,
            },
        ),
        button::Status::Pressed => (color::ACCENT, color::TEXT, color::CANVAS, Shadow::default()),
        button::Status::Disabled => (color::SURFACE, color::LINE, color::FAINT, Shadow::default()),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: border_style(border_color, 1.0, radius::CONTROL),
        shadow,
        ..button::Style::default()
    }
}

pub fn secondary_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, border_color, text_color) = match status {
        button::Status::Active => (color::SURFACE, color::LINE_STRONG, color::TEXT_SOFT),
        button::Status::Hovered => (color::SURFACE_HOVER, color::ACCENT, color::TEXT),
        button::Status::Pressed => (color::SURFACE_ACTIVE, color::ACCENT_HOVER, color::TEXT),
        button::Status::Disabled => (color::PANEL, color::LINE, color::FAINT),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: border_style(border_color, 1.0, radius::CONTROL),
        ..button::Style::default()
    }
}

pub fn navigation_button(selected: bool, status: button::Status) -> button::Style {
    if selected {
        let mut style = secondary_button(&Theme::Dark, status);
        style.background = Some(Background::Color(color::ACCENT_SOFT));
        style.text_color = color::TEXT;
        style.border.color = color::ACCENT;
        return style;
    }

    let mut style = secondary_button(&Theme::Dark, status);
    if status == button::Status::Active {
        style.background = Some(Background::Color(Color::TRANSPARENT));
        style.border.color = Color::TRANSPARENT;
        style.text_color = color::MUTED;
    }
    style
}

pub fn bare_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: color::TEXT,
        border: border_style(
            if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                color::ACCENT
            } else {
                Color::TRANSPARENT
            },
            1.0,
            3.0,
        ),
        ..button::Style::default()
    }
}

pub fn request_input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let (border_color, background) = match status {
        text_input::Status::Active => (color::LINE_STRONG, color::CANVAS),
        text_input::Status::Hovered => (color::ACCENT_SOFT, color::CANVAS_RAISED),
        text_input::Status::Focused { .. } => (color::ACCENT, color::CANVAS),
        text_input::Status::Disabled => (color::LINE, color::PANEL),
    };

    text_input::Style {
        background: Background::Color(background),
        border: border_style(border_color, 1.0, radius::CONTROL),
        icon: color::MUTED,
        placeholder: color::FAINT,
        value: color::TEXT,
        selection: color::ACCENT_SOFT,
    }
}
