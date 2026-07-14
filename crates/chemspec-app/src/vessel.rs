//! Static 2D vessel drawing for the simulation region.
//!
//! Presentation scaffolding only (`SP-003`): it renders the canonical
//! initial state — the four dissolved ion species dispersed in water — on
//! Iced's canvas without a second device or event loop. The
//! renderer-independent model that will drive this region is task `U-103`;
//! the staged particle presentation is `U-104`.

use iced::mouse::Cursor;
use iced::widget::canvas::{self, Path, Stroke};
use iced::widget::{container, row, text};
use iced::{Center, Color, Element, Point, Rectangle, Renderer, Size, Theme};

use crate::theme::{self, color, space, type_scale};

/// Representative particles per ion species. The canonical experiment mixes
/// equal moles of every ion, so equal counts preserve the validated 1:1
/// stoichiometry.
const PARTICLES_PER_SPECIES: u32 = 12;
const PARTICLE_RADIUS: f32 = 4.5;

const SILVER: Color = Color::from_rgb(0.82, 0.85, 0.91);
const NITRATE: Color = Color::from_rgb(0.96, 0.64, 0.29);
const SODIUM: Color = Color::from_rgb(0.40, 0.64, 1.0);
const CHLORIDE: Color = Color::from_rgb(0.37, 0.82, 0.55);
const WATER: Color = Color::from_rgba(0.20, 0.46, 0.76, 0.18);
const WATER_LINE: Color = Color::from_rgba(0.43, 0.72, 1.0, 0.55);
const GLASS: Color = Color::from_rgba(0.72, 0.82, 0.91, 0.54);
const GLASS_SOFT: Color = Color::from_rgba(0.72, 0.82, 0.91, 0.16);
const GRID: Color = Color::from_rgba(0.56, 0.77, 1.0, 0.045);

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
        let entry = |label, particle_color| {
            container(
                row![
                    text("●").size(type_scale::CAPTION).color(particle_color),
                    text(label)
                        .size(type_scale::CAPTION)
                        .color(color::TEXT_SOFT),
                ]
                .spacing(space::XXS)
                .align_y(Center),
            )
            .style(theme::raised)
            .padding([space::XXS, space::XS])
        };

        row![
            entry("Ag⁺", SILVER),
            entry("Cl⁻", CHLORIDE),
            entry("Na⁺", SODIUM),
            entry("NO₃⁻", NITRATE),
        ]
        .spacing(space::XS)
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
            draw_grid(frame);

            let water = water_region(frame.size());

            frame.fill(&Path::rectangle(water.position(), water.size()), WATER);
            frame.stroke(
                &Path::line(
                    Point::new(water.x, water.y),
                    Point::new(water.x + water.width, water.y),
                ),
                Stroke::default().with_color(WATER_LINE).with_width(1.5),
            );
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
    let horizontal_margin = (size.width * 0.16).clamp(24.0, 96.0);
    let vertical_margin = (size.height * 0.08).clamp(16.0, 36.0);
    let headspace = 0.19 * size.height;

    Rectangle {
        x: horizontal_margin,
        y: vertical_margin + headspace,
        width: (size.width - 2.0 * horizontal_margin).max(0.0),
        height: (size.height - 2.0 * vertical_margin - headspace).max(0.0),
    }
}

fn draw_grid(frame: &mut canvas::Frame) {
    let size = frame.size();
    let step = 42.0;
    let stroke = Stroke::default().with_color(GRID).with_width(1.0);

    let mut x = step;
    while x < size.width {
        frame.stroke(
            &Path::line(Point::new(x, 0.0), Point::new(x, size.height)),
            stroke,
        );
        x += step;
    }

    let mut y = step;
    while y < size.height {
        frame.stroke(
            &Path::line(Point::new(0.0, y), Point::new(size.width, y)),
            stroke,
        );
        y += step;
    }
}

fn draw_walls(frame: &mut canvas::Frame, water: Rectangle) {
    let mut walls = canvas::path::Builder::new();
    walls.move_to(Point::new(water.x, water.y - 28.0));
    walls.line_to(Point::new(water.x, water.y + water.height));
    walls.line_to(Point::new(water.x + water.width, water.y + water.height));
    walls.line_to(Point::new(water.x + water.width, water.y - 28.0));

    frame.stroke(
        &walls.build(),
        Stroke::default().with_color(GLASS).with_width(2.0),
    );

    frame.stroke(
        &Path::line(
            Point::new(water.x - 6.0, water.y - 28.0),
            Point::new(water.x + water.width + 6.0, water.y - 28.0),
        ),
        Stroke::default().with_color(GLASS).with_width(2.0),
    );

    frame.stroke(
        &Path::line(
            Point::new(water.x + 8.0, water.y - 20.0),
            Point::new(water.x + 8.0, water.y + water.height - 10.0),
        ),
        Stroke::default().with_color(GLASS_SOFT).with_width(2.0),
    );
}

/// Draws one species dispersed through `region` at deterministic
/// pseudo-random positions, so every render is identical.
fn scatter(frame: &mut canvas::Frame, region: Rectangle, color: Color, species_seed: u32) {
    let inset = PARTICLE_RADIUS * 2.0;

    for index in 0..PARTICLES_PER_SPECIES {
        let width = (region.width - 2.0 * inset).max(0.0);
        let height = (region.height - 2.0 * inset).max(0.0);
        let x = region.x + inset + unit(species_seed * 97 + index * 13) * width;
        let y = region.y + inset + unit(species_seed * 131 + index * 29) * height;

        let point = Point::new(x, y);
        let halo = Color::from_rgba(color.r, color.g, color.b, 0.14);

        frame.fill(&Path::circle(point, PARTICLE_RADIUS + 3.0), halo);
        frame.fill(&Path::circle(point, PARTICLE_RADIUS), color);
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
