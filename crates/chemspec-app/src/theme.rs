//! `ChemSpec`'s visual design system.
//!
//! The palette and surface hierarchy translate the quiet, instrument-like
//! character of the `ChemistrySim` reference into reusable Iced styles. Layout
//! code should use these tokens instead of introducing one-off values.

use iced::widget::{button, container, rule, slider, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector, border};

/// Complete visual configuration for the shipped dark theme.
///
/// Iced's [`Theme`] carries its widget palette. These application-owned tokens
/// cover the additional semantic roles used by custom styles and Canvas
/// renderers. Chemistry colours stay separate from interaction and status
/// colours so a theme change cannot alter scientific meaning.
#[derive(Debug, Clone, Copy)]
pub struct ThemeTokens {
    pub colors: ColorTokens,
    pub chemistry: ChemistryColorTokens,
    pub space: SpaceTokens,
    pub radius: RadiusTokens,
    pub type_scale: TypeScaleTokens,
    pub breakpoint: BreakpointTokens,
}

#[derive(Debug, Clone, Copy)]
pub struct ColorTokens {
    pub canvas: Color,
    pub canvas_raised: Color,
    pub panel: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub surface_active: Color,
    pub text: Color,
    pub text_soft: Color,
    pub muted: Color,
    pub faint: Color,
    pub line: Color,
    pub line_strong: Color,
    pub shadow: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_soft: Color,
    pub accent_faint: Color,
    pub selection: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct ChemistryColorTokens {
    pub covalent: Color,
    pub ionic: Color,
    pub electron: Color,
    pub structural_canvas: Color,
    pub structural_panel: Color,
    pub hydrogen: Color,
    pub lithium: Color,
    pub silver: Color,
    pub carbon: Color,
    pub nitrogen: Color,
    pub oxygen: Color,
    pub fluorine: Color,
    pub sodium: Color,
    pub phosphorus: Color,
    pub sulfur: Color,
    pub chlorine: Color,
    pub iron: Color,
    pub copper: Color,
    pub bromine: Color,
    pub iodine: Color,
    pub element_default: Color,
    pub alkali_metal: Color,
    pub alkaline_earth: Color,
    pub transition_metal: Color,
    pub post_transition_metal: Color,
    pub metalloid: Color,
    pub reactive_nonmetal: Color,
    pub halogen: Color,
    pub noble_gas: Color,
    pub lanthanide: Color,
    pub actinide: Color,
}

#[derive(Debug, Clone, Copy)]
pub struct SpaceTokens {
    pub xxs: f32,
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct RadiusTokens {
    pub control: f32,
    pub panel: f32,
    pub frame: f32,
    pub pill: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct TypeScaleTokens {
    pub micro: f32,
    pub caption: f32,
    pub body: f32,
    pub body_large: f32,
    pub title: f32,
    pub display: f32,
    pub hero: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct BreakpointTokens {
    pub mobile: f32,
}

pub const LAB_DARK: ThemeTokens = ThemeTokens {
    colors: ColorTokens {
        canvas: Color::from_rgb8(0x09, 0x0B, 0x0E),
        canvas_raised: Color::from_rgb8(0x0C, 0x0F, 0x13),
        panel: Color::from_rgb8(0x10, 0x14, 0x19),
        surface: Color::from_rgb8(0x15, 0x1A, 0x20),
        surface_hover: Color::from_rgb8(0x1B, 0x22, 0x2A),
        surface_active: Color::from_rgb8(0x20, 0x28, 0x31),
        text: Color::from_rgb8(0xF4, 0xF7, 0xFA),
        text_soft: Color::from_rgb8(0xC3, 0xCE, 0xD9),
        muted: Color::from_rgb8(0x9A, 0xA6, 0xB2),
        faint: Color::from_rgb8(0x6A, 0x76, 0x82),
        line: Color::from_rgb8(0x2A, 0x32, 0x3C),
        line_strong: Color::from_rgb8(0x3B, 0x48, 0x56),
        shadow: Color::BLACK,
        // A fresh laboratory-glass green: vivid enough to guide interaction
        // without reading as fluorescent or hazardous.
        accent: Color::from_rgb8(0x72, 0xD9, 0x8D),
        accent_hover: Color::from_rgb8(0x8B, 0xE6, 0xA3),
        accent_soft: Color::from_rgb8(0x1D, 0x42, 0x2B),
        accent_faint: Color::from_rgb8(0x10, 0x27, 0x1A),
        // Input-target blue: selection must read independently of the green
        // "valid" state colour, so it never borrows the accent.
        selection: Color::from_rgb8(0x7F, 0xB4, 0xFF),
        // Validation remains a distinct teal trust signal.
        success: Color::from_rgb8(0x4D, 0xC8, 0xB0),
        warning: Color::from_rgb8(0xF1, 0xB5, 0x5A),
        danger: Color::from_rgb8(0xFB, 0x71, 0x81),
    },
    chemistry: ChemistryColorTokens {
        covalent: Color::from_rgb8(0x8F, 0xC5, 0xFF),
        ionic: Color::from_rgb8(0x7A, 0xE3, 0xB0),
        electron: Color::from_rgb8(0xB7, 0xDB, 0xFF),
        structural_canvas: Color::from_rgb8(0x07, 0x09, 0x0C),
        structural_panel: Color::from_rgb8(0x0E, 0x13, 0x19),
        hydrogen: Color::from_rgb8(0xE6, 0xED, 0xF5),
        lithium: Color::from_rgb8(0xB5, 0x94, 0xF5),
        silver: Color::from_rgb8(0xC7, 0xD4, 0xE0),
        carbon: Color::from_rgb8(0x63, 0x75, 0x8A),
        nitrogen: Color::from_rgb8(0x73, 0xA8, 0xF5),
        oxygen: Color::from_rgb8(0xF2, 0x66, 0x6B),
        fluorine: Color::from_rgb8(0x9E, 0xE0, 0x66),
        sodium: Color::from_rgb8(0xAB, 0x8A, 0xF0),
        phosphorus: Color::from_rgb8(0xF5, 0xA6, 0x55),
        sulfur: Color::from_rgb8(0xF0, 0xD8, 0x5C),
        chlorine: Color::from_rgb8(0x7A, 0xE3, 0xB0),
        iron: Color::from_rgb8(0xE0, 0x7A, 0x45),
        copper: Color::from_rgb8(0xD9, 0x93, 0x4D),
        bromine: Color::from_rgb8(0xC9, 0x6A, 0x5A),
        iodine: Color::from_rgb8(0xA8, 0x6A, 0xD9),
        element_default: Color::from_rgb8(0x9E, 0xAD, 0xBD),
        alkali_metal: Color::from_rgb8(0xE8, 0x7D, 0xB8),
        alkaline_earth: Color::from_rgb8(0xF2, 0xB5, 0x59),
        transition_metal: Color::from_rgb8(0x8F, 0xC5, 0xFF),
        post_transition_metal: Color::from_rgb8(0x8A, 0xD6, 0xDB),
        metalloid: Color::from_rgb8(0xB8, 0xA3, 0xF5),
        reactive_nonmetal: Color::from_rgb8(0x6E, 0xD6, 0x94),
        halogen: Color::from_rgb8(0x80, 0xDB, 0xBF),
        noble_gas: Color::from_rgb8(0x6E, 0xC2, 0xF0),
        lanthanide: Color::from_rgb8(0xCC, 0x99, 0xEB),
        actinide: Color::from_rgb8(0xE0, 0x85, 0xA3),
    },
    space: SpaceTokens {
        xxs: 4.0,
        xs: 8.0,
        sm: 12.0,
        md: 16.0,
        lg: 24.0,
        xl: 32.0,
    },
    radius: RadiusTokens {
        control: 8.0,
        panel: 12.0,
        frame: 16.0,
        pill: 999.0,
    },
    type_scale: TypeScaleTokens {
        micro: 10.0,
        caption: 12.0,
        body: 14.0,
        body_large: 16.0,
        title: 22.0,
        display: 30.0,
        hero: 38.0,
    },
    breakpoint: BreakpointTokens { mobile: 720.0 },
};

pub mod color {
    use iced::Color;

    use super::LAB_DARK;

    pub const CANVAS: Color = LAB_DARK.colors.canvas;
    pub const CANVAS_RAISED: Color = LAB_DARK.colors.canvas_raised;
    pub const PANEL: Color = LAB_DARK.colors.panel;
    pub const SURFACE: Color = LAB_DARK.colors.surface;
    pub const SURFACE_HOVER: Color = LAB_DARK.colors.surface_hover;
    pub const SURFACE_ACTIVE: Color = LAB_DARK.colors.surface_active;

    pub const TEXT: Color = LAB_DARK.colors.text;
    pub const TEXT_SOFT: Color = LAB_DARK.colors.text_soft;
    pub const MUTED: Color = LAB_DARK.colors.muted;
    pub const FAINT: Color = LAB_DARK.colors.faint;

    pub const LINE: Color = LAB_DARK.colors.line;
    pub const LINE_STRONG: Color = LAB_DARK.colors.line_strong;
    pub const SHADOW: Color = LAB_DARK.colors.shadow;

    pub const ACCENT: Color = LAB_DARK.colors.accent;
    pub const ACCENT_HOVER: Color = LAB_DARK.colors.accent_hover;
    pub const ACCENT_SOFT: Color = LAB_DARK.colors.accent_soft;
    pub const ACCENT_FAINT: Color = LAB_DARK.colors.accent_faint;
    pub const SELECTION: Color = LAB_DARK.colors.selection;

    pub const SUCCESS: Color = LAB_DARK.colors.success;
    pub const WARNING: Color = LAB_DARK.colors.warning;
    pub const DANGER: Color = LAB_DARK.colors.danger;
}

pub mod chemistry_color {
    use iced::Color;

    use super::LAB_DARK;

    pub const COVALENT: Color = LAB_DARK.chemistry.covalent;
    pub const IONIC: Color = LAB_DARK.chemistry.ionic;
    pub const ELECTRON: Color = LAB_DARK.chemistry.electron;
    pub const STRUCTURAL_CANVAS: Color = LAB_DARK.chemistry.structural_canvas;
    pub const STRUCTURAL_PANEL: Color = LAB_DARK.chemistry.structural_panel;
}

pub mod space {
    use super::LAB_DARK;

    pub const XXS: f32 = LAB_DARK.space.xxs;
    pub const XS: f32 = LAB_DARK.space.xs;
    pub const SM: f32 = LAB_DARK.space.sm;
    pub const MD: f32 = LAB_DARK.space.md;
    pub const LG: f32 = LAB_DARK.space.lg;
    pub const XL: f32 = LAB_DARK.space.xl;
}

pub mod radius {
    use super::LAB_DARK;

    pub const CONTROL: f32 = LAB_DARK.radius.control;
    pub const PANEL: f32 = LAB_DARK.radius.panel;
    pub const FRAME: f32 = LAB_DARK.radius.frame;
    pub const PILL: f32 = LAB_DARK.radius.pill;
}

pub mod type_scale {
    use super::LAB_DARK;

    pub const MICRO: f32 = LAB_DARK.type_scale.micro;
    pub const CAPTION: f32 = LAB_DARK.type_scale.caption;
    pub const BODY: f32 = LAB_DARK.type_scale.body;
    pub const BODY_LARGE: f32 = LAB_DARK.type_scale.body_large;
    pub const TITLE: f32 = LAB_DARK.type_scale.title;
    pub const DISPLAY: f32 = LAB_DARK.type_scale.display;
    pub const HERO: f32 = LAB_DARK.type_scale.hero;
}

/// Interface motion cadence shared by animated views.
///
/// One tick advances hover reveals and the slow orbital phase; subscriptions
/// must only run while something on screen actually moves.
pub mod motion {
    /// Frame cadence for continuous interface motion.
    pub const TICK: std::time::Duration = std::time::Duration::from_millis(33);
    /// Frame cadence for the builder prompt's opacity transition.
    pub const PROMPT_TICK: std::time::Duration = std::time::Duration::from_millis(16);
    /// Per-frame prompt fade progress (~400 ms to complete).
    pub const PROMPT_FADE_STEP: f32 = 0.04;
    /// Per-tick orbital advance (one revolution ≈ 12.5 s).
    pub const ORBIT_STEP: f32 = 0.002_6;
    /// Per-tick progress of a hold-to-clear gesture (~650 ms to complete).
    pub const HOLD_CLEAR_STEP: f32 = 0.051;
    /// Per-tick progress of an element key's release fade (~1.1 s).
    pub const KEY_RELEASE_STEP: f32 = 0.03;
    /// Per-tick progress of a hover state's release fade (~180 ms).
    pub const HOVER_RELEASE_STEP: f32 = 0.18;
}

/// Symmetric smoothstep easing for opacity transitions in either direction.
pub fn ease_in_out(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

/// Linear interpolation between two colours, used for fading emphasis.
pub fn mix(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color {
        r: from.r + (to.r - from.r) * amount,
        g: from.g + (to.g - from.g) * amount,
        b: from.b + (to.b - from.b) * amount,
        a: from.a + (to.a - from.a) * amount,
    }
}

pub mod breakpoint {
    use super::LAB_DARK;

    pub const MOBILE: f32 = LAB_DARK.breakpoint.mobile;
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

pub fn frame(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::PANEL)
        .border(border_style(color::LINE, 1.0, radius::FRAME))
        .shadow(Shadow {
            color: color::SHADOW.scale_alpha(0.32),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        })
}

pub fn inset(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::CANVAS)
        .border(border_style(color::LINE_STRONG, 1.0, radius::CONTROL))
}

/// Dimmed backdrop behind the dynamic-build modal.
pub fn overlay_scrim(_: &Theme) -> container::Style {
    container::Style::default().background(Color {
        a: 0.55,
        ..Color::BLACK
    })
}

/// The dynamic-build modal panel itself.
pub fn overlay_panel(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::PANEL)
        .border(border_style(color::LINE_STRONG, 1.0, radius::FRAME))
        .shadow(Shadow {
            color: color::SHADOW.scale_alpha(0.45),
            offset: Vector::new(0.0, 18.0),
            blur_radius: 48.0,
        })
}

pub fn media_bar(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::CANVAS_RAISED.scale_alpha(0.98))
        .border(border_style(color::LINE_STRONG, 1.0, radius::PANEL))
        .shadow(Shadow {
            color: color::SHADOW.scale_alpha(0.22),
            offset: Vector::new(0.0, 5.0),
            blur_radius: 18.0,
        })
}

pub fn summary_visual_panel(_: &Theme) -> container::Style {
    container::Style::default()
        .background(chemistry_color::STRUCTURAL_CANVAS)
        .border(border_style(color::LINE_STRONG, 1.0, radius::PANEL))
        .shadow(Shadow {
            color: color::SHADOW.scale_alpha(0.28),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        })
}

pub fn summary_properties_panel(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::CANVAS_RAISED)
        .border(border_style(color::LINE_STRONG, 1.0, radius::PANEL))
        .shadow(Shadow {
            color: color::SHADOW.scale_alpha(0.30),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 28.0,
        })
}

pub fn summary_property_row(started: bool, active: bool) -> container::Style {
    let background = if active {
        color::ACCENT_FAINT.scale_alpha(0.74)
    } else if started {
        color::SURFACE.scale_alpha(0.66)
    } else {
        color::CANVAS.scale_alpha(0.44)
    };
    let border = if active {
        color::ACCENT.scale_alpha(0.62)
    } else {
        color::LINE.scale_alpha(0.82)
    };
    container::Style::default()
        .background(background)
        .border(border_style(border, 1.0, radius::CONTROL))
}

pub fn summary_more_info_panel(_: &Theme) -> container::Style {
    container::Style::default()
        .background(color::ACCENT_FAINT.scale_alpha(0.52))
        .border(border_style(
            color::ACCENT.scale_alpha(0.34),
            1.0,
            radius::CONTROL,
        ))
}

#[must_use]
pub fn summary_chat_message(is_user: bool) -> container::Style {
    let background = if is_user {
        color::ACCENT_FAINT.scale_alpha(0.58)
    } else {
        color::SURFACE.scale_alpha(0.82)
    };
    let border = if is_user {
        color::ACCENT.scale_alpha(0.30)
    } else {
        color::LINE.scale_alpha(0.82)
    };
    container::Style::default()
        .background(background)
        .border(border_style(border, 1.0, radius::CONTROL))
}

pub fn timeline_slider(_: &Theme, status: slider::Status) -> slider::Style {
    let (radius, handle, border_color, rail) = match status {
        slider::Status::Active => (7.0, color::TEXT, color::CANVAS, color::ACCENT),
        slider::Status::Hovered => (8.5, color::ACCENT_HOVER, color::TEXT, color::ACCENT_HOVER),
        slider::Status::Dragged => (9.0, color::TEXT, color::ACCENT, color::ACCENT_HOVER),
    };

    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                Background::Color(rail),
                Background::Color(color::SURFACE_ACTIVE),
            ),
            width: 4.0,
            border: border_style(color::LINE_STRONG, 1.0, radius::PILL),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius },
            background: Background::Color(handle),
            border_width: 2.5,
            border_color,
        },
    }
}

/// A reactant slot chip inside the question sentence.
///
/// The border colour carries the draft's state (green valid, orange
/// unrecognised, grey empty); the selected slot adds a blue tint and thicker
/// border so selection reads independently of state. The words behind each
/// colour live in the slot tooltip.
pub fn slot_chip(state_color: Color, selected: bool, hovered: bool) -> container::Style {
    let background = if selected {
        color::SELECTION.scale_alpha(0.12)
    } else if hovered {
        color::SURFACE_HOVER
    } else {
        color::PANEL
    };

    container::Style::default()
        .background(background)
        .border(border_style(
            state_color,
            if selected { 1.5 } else { 1.0 },
            radius::PANEL,
        ))
}

/// The hover tooltip that carries an atomic or compound model.
///
/// `reveal` fades the surface in over the motion tokens' reveal window; the
/// diagrams inside fade with the same progress.
pub fn tooltip_surface(reveal: f32) -> container::Style {
    container::Style::default()
        .background(Color {
            a: reveal,
            ..color::SURFACE
        })
        .color(Color {
            a: reveal,
            ..color::TEXT
        })
        .border(border_style(
            color::LINE_STRONG.scale_alpha(reveal),
            1.0,
            radius::PANEL,
        ))
        .shadow(Shadow {
            color: color::SHADOW.scale_alpha(0.45 * reveal),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 28.0,
        })
}

/// Periodic-family colour shared by element keys, drags, and tooltips.
pub const fn category_color(category: crate::elements::Category) -> Color {
    use crate::elements::Category;
    match category {
        Category::AlkaliMetal => LAB_DARK.chemistry.alkali_metal,
        Category::AlkalineEarth => LAB_DARK.chemistry.alkaline_earth,
        Category::TransitionMetal => LAB_DARK.chemistry.transition_metal,
        Category::PostTransitionMetal => LAB_DARK.chemistry.post_transition_metal,
        Category::Metalloid => LAB_DARK.chemistry.metalloid,
        Category::ReactiveNonmetal => LAB_DARK.chemistry.reactive_nonmetal,
        Category::Halogen => LAB_DARK.chemistry.halogen,
        Category::NobleGas => LAB_DARK.chemistry.noble_gas,
        Category::Lanthanide => LAB_DARK.chemistry.lanthanide,
        Category::Actinide => LAB_DARK.chemistry.actinide,
    }
}

pub fn provider_button(selected: bool, status: button::Status) -> button::Style {
    let (background, border_color, text_color) = if selected {
        match status {
            button::Status::Active => (color::ACCENT_FAINT, color::ACCENT, color::TEXT),
            button::Status::Hovered => (color::ACCENT_SOFT, color::ACCENT_HOVER, color::TEXT),
            button::Status::Pressed => (color::SURFACE_ACTIVE, color::ACCENT, color::TEXT),
            button::Status::Disabled => (color::PANEL, color::LINE, color::FAINT),
        }
    } else {
        match status {
            button::Status::Active => (color::SURFACE, color::LINE_STRONG, color::TEXT),
            button::Status::Hovered => (color::SURFACE_HOVER, color::ACCENT, color::TEXT),
            button::Status::Pressed => (color::SURFACE_ACTIVE, color::ACCENT_HOVER, color::TEXT),
            button::Status::Disabled => (color::CANVAS_RAISED, color::LINE, color::FAINT),
        }
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: border_style(border_color, 1.0, radius::CONTROL),
        ..button::Style::default()
    }
}

pub fn soft_divider(_: &Theme) -> rule::Style {
    rule::Style {
        color: color::LINE,
        radius: border::Radius::default(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub fn danger_divider(_: &Theme) -> rule::Style {
    rule::Style {
        color: color::DANGER,
        radius: border::Radius::default(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub fn soft_rule(_: &Theme) -> rule::Style {
    rule::Style {
        color: color::LINE,
        radius: border::Radius::new(radius::PILL),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

pub fn primary_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, border_color, text_color, shadow) = match status {
        button::Status::Active => (
            color::ACCENT,
            color::ACCENT,
            color::CANVAS,
            Shadow {
                color: color::ACCENT.scale_alpha(0.10),
                offset: Vector::new(0.0, 2.0),
                blur_radius: 8.0,
            },
        ),
        button::Status::Hovered => (
            color::ACCENT_HOVER,
            color::ACCENT_HOVER,
            color::CANVAS,
            Shadow {
                color: color::ACCENT.scale_alpha(0.16),
                offset: Vector::new(0.0, 3.0),
                blur_radius: 10.0,
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

/// Text-only submit affordance used beneath the builder question.
///
/// It shares the question's typography and changes only foreground colour on
/// hover or press, while `reveal` supplies the two-way opacity transition.
pub fn run_prompt(_: &Theme, status: button::Status, reveal: f32) -> button::Style {
    let reveal = reveal.clamp(0.0, 1.0);
    let text_color = match status {
        button::Status::Active | button::Status::Disabled => color::TEXT_SOFT.scale_alpha(reveal),
        button::Status::Hovered => color::ACCENT.scale_alpha(reveal),
        button::Status::Pressed => color::ACCENT_HOVER.scale_alpha(reveal),
    };

    button::Style {
        text_color,
        ..button::Style::default()
    }
}

/// An invisible button wrapper: the content inside carries all the visual
/// feedback (the element keys style their own hover, release, and drag).
pub fn bare_button(_: &Theme, _: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: color::TEXT,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_channel(channel: f32) -> f32 {
        if channel <= 0.040_45 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(color: Color) -> f32 {
        0.2126 * linear_channel(color.r)
            + 0.7152 * linear_channel(color.g)
            + 0.0722 * linear_channel(color.b)
    }

    fn contrast_ratio(left: Color, right: Color) -> f32 {
        let light = luminance(left).max(luminance(right));
        let dark = luminance(left).min(luminance(right));
        (light + 0.05) / (dark + 0.05)
    }

    fn color_distance_squared(left: Color, right: Color) -> f32 {
        (left.r - right.r).powi(2) + (left.g - right.g).powi(2) + (left.b - right.b).powi(2)
    }

    #[test]
    fn iced_palette_is_derived_from_semantic_tokens() {
        let palette = app_theme().palette();
        assert_eq!(palette.background, color::CANVAS);
        assert_eq!(palette.text, color::TEXT);
        assert_eq!(palette.primary, color::ACCENT);
        assert_eq!(palette.success, color::SUCCESS);
        assert_eq!(palette.warning, color::WARNING);
        assert_eq!(palette.danger, color::DANGER);
    }

    #[test]
    fn primary_actions_have_strong_dark_theme_contrast() {
        assert!(contrast_ratio(color::ACCENT, color::CANVAS) >= 7.0);
        assert!(contrast_ratio(color::ACCENT_HOVER, color::CANVAS) >= 7.0);
        assert!(contrast_ratio(color::TEXT, color::CANVAS) >= 7.0);
    }

    #[test]
    fn accent_and_validation_are_visually_distinct() {
        assert!(color_distance_squared(color::ACCENT, color::SUCCESS) >= 0.03);
        assert!(contrast_ratio(color::SUCCESS, color::CANVAS) >= 7.0);
    }
}
