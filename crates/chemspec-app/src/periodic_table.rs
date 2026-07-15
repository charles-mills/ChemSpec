//! Stage 1 of the reaction builder: periodic-table discovery and selection.
//!
//! The table renders as a quiet keyboard: every key carries its atomic
//! number, symbol, full element name, and a periodic-family colour tick, so
//! no separate legend or instruction strip is required.

use iced::event;
use iced::mouse;
use iced::widget::{button, column, container, mouse_area, responsive, row, space, text};
use iced::{
    Background, Border, Center, Color, Element, Fill, Length, Padding, Point, Shadow, Size,
    Subscription, Vector, border,
};

use crate::elements::{self, Category, ElementSpec};
use crate::theme::{self, color, radius, space as spacing, type_scale};

const DISPLAY_ROWS: usize = 9;
const GROUPS: usize = 18;
const DISPLAY_ROWS_F32: f32 = 9.0;
const GROUPS_F32: f32 = 18.0;
const TABLE_GAPS: f32 = 17.0;
const MIN_CELL_WIDTH: f32 = 18.0;
const MAX_CELL_WIDTH: f32 = 64.0;
/// Keys are slightly wider than tall, like keycaps.
const CELL_ASPECT: f32 = 0.84;
const DRAG_WIDTH: f32 = 92.0;
const DRAG_HEIGHT: f32 = 78.0;

#[derive(Debug, Clone, Copy)]
struct TableGeometry {
    cell_width: f32,
    cell_height: f32,
    group_gap: f32,
    block_gap: f32,
    row_gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DragState {
    atomic_number: u8,
    pointer: Point,
    positioned: bool,
}

/// An activated key's emphasis, fading out like a released keyboard key.
#[derive(Debug, Clone, Copy)]
struct Release {
    atomic_number: u8,
    progress: f32,
}

impl Release {
    /// Quadratic tail: bright on press, then a long soft fade.
    fn intensity(self) -> f32 {
        let remaining = (1.0 - self.progress).clamp(0.0, 1.0);
        remaining * remaining
    }
}

#[derive(Debug, Default)]
pub struct State {
    hovered: Option<u8>,
    dragging: Option<DragState>,
    releasing: Option<Release>,
    /// The last un-hovered key, easing back to rest.
    hover_fading: Option<Release>,
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    HoverChanged(Option<u8>),
    DragStarted(u8),
    Activated(u8),
    DragMoved(Point),
    DragEnded,
    ReleaseTick,
}

pub fn update(state: &mut State, message: Message) {
    match message {
        Message::HoverChanged(hovered) => {
            if let Some(previous) = state.hovered.filter(|previous| Some(*previous) != hovered) {
                state.hover_fading = Some(Release {
                    atomic_number: previous,
                    progress: 0.0,
                });
            }
            if state
                .hover_fading
                .is_some_and(|fade| Some(fade.atomic_number) == hovered)
            {
                state.hover_fading = None;
            }
            state.hovered = hovered;
        }
        Message::DragStarted(atomic_number) => {
            state.releasing = Some(Release {
                atomic_number,
                progress: 0.0,
            });
            state.dragging = Some(DragState {
                atomic_number,
                pointer: Point::ORIGIN,
                positioned: false,
            });
        }
        Message::Activated(atomic_number) => {
            state.releasing = Some(Release {
                atomic_number,
                progress: 0.0,
            });
            state.dragging = None;
        }
        Message::DragMoved(pointer) => {
            if let Some(dragging) = &mut state.dragging {
                dragging.pointer = pointer;
                dragging.positioned = true;
            }
        }
        Message::DragEnded => state.dragging = None,
        Message::ReleaseTick => {
            if let Some(release) = &mut state.releasing {
                release.progress += theme::motion::KEY_RELEASE_STEP;
                if release.progress >= 1.0 {
                    state.releasing = None;
                }
            }
            if let Some(fade) = &mut state.hover_fading {
                fade.progress += theme::motion::HOVER_RELEASE_STEP;
                if fade.progress >= 1.0 {
                    state.hover_fading = None;
                }
            }
        }
    }
}

pub fn subscription(state: &State) -> Subscription<Message> {
    let drag = state.dragging.is_some().then(|| {
        event::listen_with(|event, _status, _window| match event {
            iced::Event::Mouse(mouse::Event::CursorMoved { position })
            | iced::Event::Touch(iced::touch::Event::FingerMoved { position, .. }) => {
                Some(Message::DragMoved(position))
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | iced::Event::Touch(
                iced::touch::Event::FingerLifted { .. } | iced::touch::Event::FingerLost { .. },
            ) => Some(Message::DragEnded),
            _ => None,
        })
    });
    let release = (state.releasing.is_some() || state.hover_fading.is_some())
        .then(|| iced::time::every(theme::motion::TICK).map(|_| Message::ReleaseTick));

    Subscription::batch(drag.into_iter().chain(release))
}

pub fn dragging_atomic_number(state: &State) -> Option<u8> {
    state.dragging.map(|drag| drag.atomic_number)
}

/// Renders the active library drag at window coordinates so it can cross every
/// application surface without being clipped by the periodic-table panel.
pub fn drag_overlay(state: &State, size: Size) -> Element<'static, Message> {
    let Some(drag) = state.dragging.filter(|drag| drag.positioned) else {
        return space().into();
    };
    let Some(element) = elements::by_atomic_number(drag.atomic_number) else {
        return space().into();
    };
    let left = (drag.pointer.x - DRAG_WIDTH / 2.0).clamp(0.0, (size.width - DRAG_WIDTH).max(0.0));
    let top = (drag.pointer.y - DRAG_HEIGHT / 2.0).clamp(0.0, (size.height - DRAG_HEIGHT).max(0.0));
    let accent = theme::category_color(element.category);
    let preview = container(
        column![
            text(element.atomic_number.to_string())
                .size(type_scale::MICRO)
                .color(color::TEXT_SOFT),
            text(element.symbol)
                .size(type_scale::TITLE)
                .color(color::TEXT),
            text(element.name)
                .size(type_scale::MICRO)
                .color(color::TEXT_SOFT),
        ]
        .spacing(spacing::XXS),
    )
    .style(move |_| drag_style(accent))
    .padding([spacing::XXS, spacing::XS])
    .width(Length::Fixed(DRAG_WIDTH))
    .height(Length::Fixed(DRAG_HEIGHT));

    container(preview)
        .padding(Padding {
            top,
            right: 0.0,
            bottom: 0.0,
            left,
        })
        .width(Fill)
        .height(Fill)
        .into()
}

pub fn view(state: &State, _compact: bool) -> Element<'_, Message> {
    responsive(move |size| periodic_grid(state, size))
        .height(Fill)
        .into()
}

fn periodic_grid(state: &State, available: Size) -> Element<'static, Message> {
    let geometry = table_geometry(available.width, available.height);

    let mut grid = column![].spacing(geometry.row_gap);
    for period in 1..=DISPLAY_ROWS {
        let mut period_row = row![];
        for group in 1..=GROUPS {
            let element = elements::SUPPORTED.iter().find(|element| {
                let (row, column) = elements::display_position(**element);
                usize::from(row) == period && usize::from(column) == group
            });

            period_row = period_row.push(match element {
                Some(element) => element_tile(state, *element, geometry),
                None if period == 6 && group == 3 => series_placeholder("57–71", geometry),
                None if period == 7 && group == 3 => series_placeholder("89–103", geometry),
                None => empty_cell(geometry),
            });
            period_row = push_group_gap(period_row, group, geometry);
        }
        grid = grid.push(period_row);
    }

    container(grid.width(Length::Shrink))
        .center_x(Fill)
        .center_y(Fill)
        .into()
}

fn push_group_gap(
    row: iced::widget::Row<'_, Message>,
    group: usize,
    geometry: TableGeometry,
) -> iced::widget::Row<'_, Message> {
    if group >= GROUPS {
        row
    } else {
        let gap = if group == 2 || group == 12 {
            geometry.block_gap
        } else {
            geometry.group_gap
        };
        row.push(space().width(Length::Fixed(gap)))
    }
}

fn element_tile(
    state: &State,
    element: ElementSpec,
    geometry: TableGeometry,
) -> Element<'static, Message> {
    let release = state
        .releasing
        .filter(|release| release.atomic_number == element.atomic_number)
        .map(Release::intensity);
    let hover_fade = state
        .hover_fading
        .filter(|fade| fade.atomic_number == element.atomic_number)
        .map(|fade| 1.0 - fade.progress.clamp(0.0, 1.0));
    let dragging = state
        .dragging
        .is_some_and(|drag| drag.atomic_number == element.atomic_number);
    let hovered = state.hovered == Some(element.atomic_number);
    let family = theme::category_color(element.category);
    let very_dense = geometry.cell_width < 26.0;
    let named = geometry.cell_width >= 48.0;
    let lifted = hovered || dragging || release.is_some();
    let secondary = if lifted {
        color::TEXT_SOFT
    } else {
        color::MUTED
    };
    let tick_alpha = if lifted {
        1.0
    } else {
        0.6 + 0.4 * hover_fade.unwrap_or(0.0)
    };

    let symbol = text(element.symbol)
        .size(if very_dense {
            11.0
        } else if named {
            15.0
        } else {
            13.0
        })
        .color(color::TEXT)
        .width(Fill)
        .align_x(Center);

    let mut key = column![];
    if !very_dense {
        key = key.push(
            text(element.atomic_number.to_string())
                .size(7)
                .color(secondary),
        );
    }
    key = key.push(symbol);
    if named {
        key = key.push(
            text(element.name)
                .size(7)
                .color(secondary)
                .width(Fill)
                .align_x(Center),
        );
    }
    key = key.push(space().height(Fill));
    key = key.push(
        container(space())
            .width(Fill)
            .height(Length::Fixed(2.0))
            .style(move |_| family_tick(family.scale_alpha(tick_alpha))),
    );

    let tile = container(key.spacing(0).width(Fill))
        .padding(Padding {
            top: 2.0,
            right: 4.0,
            bottom: 3.0,
            left: 4.0,
        })
        .width(Length::Fixed(geometry.cell_width))
        .height(Length::Fixed(geometry.cell_height))
        .style(move |_| {
            let emphasis = if dragging {
                TileEmphasis::Dragging
            } else if let Some(intensity) = release {
                TileEmphasis::Released { intensity, hovered }
            } else if hovered {
                TileEmphasis::Hovered
            } else if let Some(intensity) = hover_fade {
                TileEmphasis::HoverFading(intensity)
            } else {
                TileEmphasis::Idle
            };

            tile_style(element.category, emphasis)
        });

    let accessible_tile = button(tile)
        .on_press(Message::Activated(element.atomic_number))
        .padding(0)
        .style(theme::bare_button);

    mouse_area(accessible_tile)
        .on_press(Message::DragStarted(element.atomic_number))
        .on_enter(Message::HoverChanged(Some(element.atomic_number)))
        .on_exit(Message::HoverChanged(None))
        .interaction(if dragging {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Grab
        })
        .into()
}

fn empty_cell(geometry: TableGeometry) -> Element<'static, Message> {
    container(space())
        .width(Length::Fixed(geometry.cell_width))
        .height(Length::Fixed(geometry.cell_height))
        .into()
}

fn series_placeholder(label: &'static str, geometry: TableGeometry) -> Element<'static, Message> {
    container(
        container(text(label).size(8).color(color::FAINT))
            .center_x(Fill)
            .center_y(Fill),
    )
    .style(series_marker)
    .width(Length::Fixed(geometry.cell_width))
    .height(Length::Fixed(geometry.cell_height))
    .into()
}

fn table_geometry(available_width: f32, available_height: f32) -> TableGeometry {
    let group_gap = if available_width < 900.0 { 2.0 } else { 4.0 };
    let row_gap = group_gap;
    let usable_width = (available_width - spacing::MD * 2.0).max(0.0);
    let usable_height = (available_height - spacing::SM * 2.0).max(0.0);
    let responsive_cap = if available_width < 720.0 {
        24.0
    } else if available_width < 1_120.0 {
        40.0
    } else {
        MAX_CELL_WIDTH
    };
    let width_bound = (usable_width - TABLE_GAPS * group_gap) / GROUPS_F32;
    let height_bound = ((usable_height - 8.0 * row_gap) / DISPLAY_ROWS_F32) / CELL_ASPECT;
    let cell_width = width_bound
        .min(height_bound)
        .clamp(MIN_CELL_WIDTH, responsive_cap);
    let cell_height = cell_width * CELL_ASPECT;
    // The gaps after groups 2 and 12 stay slightly wider than the group
    // rhythm so the s, d, and p blocks read as families, without stretching
    // the table to fill the panel.
    let block_gap = group_gap * 3.0;

    TableGeometry {
        cell_width,
        cell_height,
        group_gap,
        block_gap,
        row_gap,
    }
}

#[derive(Debug, Clone, Copy)]
enum TileEmphasis {
    Idle,
    Hovered,
    HoverFading(f32),
    Released { intensity: f32, hovered: bool },
    Dragging,
}

fn tile_style(category: Category, emphasis: TileEmphasis) -> container::Style {
    let accent = theme::category_color(category);

    // A hovered key wakes up in its own family colour: the surface and
    // border lift toward it and the tick brightens. No drop shadow — at key
    // size on a dark canvas it reads as smear, not depth.
    let hover_background = theme::mix(color::SURFACE, accent, 0.10);
    let hover_border = theme::mix(color::LINE, accent, 0.60);

    let (background, border_color, border_width, shadow) = match emphasis {
        TileEmphasis::Idle => (color::SURFACE, color::LINE, 1.0, Shadow::default()),
        TileEmphasis::Hovered => (hover_background, hover_border, 1.0, Shadow::default()),
        TileEmphasis::HoverFading(intensity) => (
            theme::mix(color::SURFACE, hover_background, intensity),
            theme::mix(color::LINE, hover_border, intensity),
            1.0,
            Shadow::default(),
        ),
        // A just-pressed key settles back to its resting (or hovered) look.
        TileEmphasis::Released { intensity, hovered } => {
            let (base_background, base_border) = if hovered {
                (hover_background, hover_border)
            } else {
                (color::SURFACE, color::LINE)
            };
            (
                theme::mix(base_background, accent, 0.18 * intensity),
                theme::mix(base_border, accent, intensity),
                1.0 + 0.5 * intensity,
                Shadow::default(),
            )
        }
        TileEmphasis::Dragging => (
            accent.scale_alpha(0.24),
            accent,
            2.0,
            Shadow {
                color: accent.scale_alpha(0.28),
                offset: Vector::new(0.0, 6.0),
                blur_radius: 16.0,
            },
        ),
    };

    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(color::TEXT),
        border: Border {
            color: border_color,
            width: border_width,
            radius: border::Radius::new(5.0),
        },
        shadow,
        ..container::Style::default()
    }
}

fn family_tick(family: Color) -> container::Style {
    container::Style::default()
        .background(family)
        .border(Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: border::Radius::new(1.0),
        })
}

fn series_marker(_: &iced::Theme) -> container::Style {
    container::Style::default().border(Border {
        color: color::LINE,
        width: 1.0,
        radius: border::Radius::new(5.0),
    })
}

fn drag_style(accent: Color) -> container::Style {
    container::Style::default()
        .background(accent.scale_alpha(0.25))
        .border(Border {
            color: accent,
            width: 2.0,
            radius: border::Radius::new(radius::CONTROL),
        })
        .shadow(Shadow {
            color: color::SHADOW.scale_alpha(0.48),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 22.0,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_width(geometry: TableGeometry) -> f32 {
        GROUPS_F32 * geometry.cell_width
            + (TABLE_GAPS - 2.0) * geometry.group_gap
            + 2.0 * geometry.block_gap
    }

    fn table_height(geometry: TableGeometry) -> f32 {
        DISPLAY_ROWS_F32 * geometry.cell_height + 8.0 * geometry.row_gap
    }

    #[test]
    fn drag_flashes_the_key_and_global_release_cancels_it() {
        let mut state = State::default();

        update(&mut state, Message::DragStarted(8));
        update(&mut state, Message::DragMoved(Point::new(120.0, 80.0)));
        assert!(matches!(state.releasing, Some(release) if release.atomic_number == 8));
        assert!(state.dragging.is_some());

        update(&mut state, Message::DragEnded);
        assert!(state.dragging.is_none());
    }

    #[test]
    fn leaving_a_key_fades_its_hover_state_out() {
        let mut state = State::default();
        update(&mut state, Message::HoverChanged(Some(8)));
        assert!(state.hover_fading.is_none());

        update(&mut state, Message::HoverChanged(None));
        assert!(matches!(state.hover_fading, Some(fade) if fade.atomic_number == 8));

        // Re-entering the same key cancels its fade-out.
        update(&mut state, Message::HoverChanged(Some(8)));
        assert!(state.hover_fading.is_none());

        update(&mut state, Message::HoverChanged(None));
        for _ in 0..200 {
            if state.hover_fading.is_none() {
                break;
            }
            update(&mut state, Message::ReleaseTick);
        }
        assert!(state.hover_fading.is_none());
    }

    #[test]
    fn an_activated_key_fades_out_like_a_released_key() {
        let mut state = State::default();
        update(&mut state, Message::Activated(3));
        let full = state.releasing.expect("release starts").intensity();
        assert!((full - 1.0).abs() < f32::EPSILON);

        update(&mut state, Message::ReleaseTick);
        let fading = state.releasing.expect("release fades").intensity();
        assert!(fading < full && fading > 0.0);

        // The fade always finishes and stops requesting ticks, regardless of
        // the exact motion-token duration.
        for _ in 0..200 {
            if state.releasing.is_none() {
                break;
            }
            update(&mut state, Message::ReleaseTick);
        }
        assert!(state.releasing.is_none());
    }

    #[test]
    fn table_geometry_always_fits_the_available_area_without_scrolling() {
        let compact = table_geometry(620.0, 400.0);
        let desktop = table_geometry(1_360.0, 620.0);
        let wide = table_geometry(1_900.0, 700.0);
        let short = table_geometry(1_360.0, 300.0);

        for geometry in [compact, desktop, wide, short] {
            assert!((geometry.cell_height - geometry.cell_width * CELL_ASPECT).abs() < 0.01);
            assert!(geometry.block_gap > geometry.group_gap);
        }
        assert!(table_width(compact) <= 620.0);
        assert!(table_width(desktop) <= 1_360.0);
        assert!(table_width(wide) <= 1_900.0);
        assert!(desktop.cell_width > compact.cell_width);
        assert!(wide.cell_width <= MAX_CELL_WIDTH);
        // A short panel bounds the cell size by height instead of width.
        assert!(short.cell_width < desktop.cell_width);
        assert!(table_height(short) <= 300.0 - spacing::SM * 2.0 + 0.01);
        assert!((desktop.group_gap - desktop.row_gap).abs() < f32::EPSILON);
    }
}
