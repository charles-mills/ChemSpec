//! Static 2D vessel drawing for the simulation region.
//!
//! Presentation scaffolding only (`SP-003`): it renders the canonical
//! initial state — the four dissolved ion species dispersed in water — on
//! Iced's canvas without a second device or event loop. The
//! renderer-independent model that will drive this region is task `U-103`;
//! the staged particle presentation is `U-104`.

use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::widget::{row, text};
use iced::{Center, Color, Element, Point, Rectangle, Renderer, Size, Theme};

/// Representative particles per ion species. The canonical experiment mixes
/// equal moles of every ion, so equal counts preserve the validated 1:1
/// stoichiometry.
const PARTICLES_PER_SPECIES: u32 = 12;
const PARTICLE_RADIUS: f32 = 5.0;

const SILVER: Color = Color::from_rgb(0.78, 0.78, 0.85);
const NITRATE: Color = Color::from_rgb(0.90, 0.58, 0.25);
const SODIUM: Color = Color::from_rgb(0.38, 0.56, 0.92);
const CHLORIDE: Color = Color::from_rgb(0.38, 0.78, 0.45);
const WATER: Color = Color::from_rgba(0.25, 0.45, 0.75, 0.16);
const GLASS: Color = Color::from_rgb(0.55, 0.60, 0.70);

pub struct Vessel {
    cache: canvas::Cache,
}

impl Vessel {
    pub fn new() -> Self {
        Self {
            cache: canvas::Cache::new(),
        }
    }

    pub fn legend<Message: 'static>() -> Element<'static, Message> {
        let entry = |label, color| text(label).size(12).color(color);

        row![
            entry("● Ag+", SILVER),
            entry("● Cl-", CHLORIDE),
            entry("● Na+", SODIUM),
            entry("● NO3-", NITRATE),
        ]
        .spacing(12)
        .align_y(Center)
        .into()
    }
}

impl<Message> canvas::Program<Message> for Vessel {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<canvas::Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let water = water_region(frame.size());

            frame.fill(&Path::rectangle(water.position(), water.size()), WATER);
            draw_walls(frame, water);

            scatter(frame, water, SILVER, 1);
            scatter(frame, water, NITRATE, 2);
            scatter(frame, water, SODIUM, 3);
            scatter(frame, water, CHLORIDE, 4);
        });

        vec![geometry]
    }
}

fn water_region(size: Size) -> Rectangle {
    let margin = 18.0;
    let headspace = 0.18 * size.height;

    Rectangle {
        x: margin,
        y: margin + headspace,
        width: (size.width - 2.0 * margin).max(0.0),
        height: (size.height - 2.0 * margin - headspace).max(0.0),
    }
}

fn draw_walls(frame: &mut canvas::Frame, water: Rectangle) {
    let mut walls = canvas::path::Builder::new();
    walls.move_to(Point::new(water.x, water.y - 12.0));
    walls.line_to(Point::new(water.x, water.y + water.height));
    walls.line_to(Point::new(water.x + water.width, water.y + water.height));
    walls.line_to(Point::new(water.x + water.width, water.y - 12.0));

    frame.stroke(
        &walls.build(),
        Stroke::default().with_color(GLASS).with_width(2.0),
    );
}

/// Draws one species dispersed through `region` at deterministic
/// pseudo-random positions, so every render is identical.
fn scatter(frame: &mut canvas::Frame, region: Rectangle, color: Color, species_seed: u32) {
    let inset = PARTICLE_RADIUS * 2.0;

    for index in 0..PARTICLES_PER_SPECIES {
        let x =
            region.x + inset + unit(species_seed * 97 + index * 13) * (region.width - 2.0 * inset);
        let y = region.y
            + inset
            + unit(species_seed * 131 + index * 29) * (region.height - 2.0 * inset);

        frame.fill(&Path::circle(Point::new(x, y), PARTICLE_RADIUS), color);
    }
}

/// Deterministic hash of `seed` into `[0, 1)`.
fn unit(seed: u32) -> f32 {
    let hashed = seed
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223)
        .rotate_left(13)
        .wrapping_mul(2_654_435_761);

    f32::from_bits(0x3f80_0000 | (hashed >> 9)) - 1.0
}
