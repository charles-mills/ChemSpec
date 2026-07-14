//! Stage 1 of the reaction builder: periodic-table discovery and selection.

use iced::event;
use iced::mouse;
use iced::widget::{Grid, column, container, mouse_area, responsive, row, space, text};
use iced::{
    Background, Border, Color, Element, Fill, Length, Padding, Point, Shadow, Size, Subscription,
    Vector, border,
};

use crate::elements::{self, Category, ElementSpec};
use crate::theme::{self, color, radius, space as spacing, type_scale};

const PERIODS: usize = 5;
const GROUPS: usize = 18;
const PERIODS_F32: f32 = 5.0;
const GROUPS_F32: f32 = 18.0;
const TABLE_GAPS: f32 = 17.0;
const MIN_CELL_WIDTH: f32 = 18.0;
const MAX_CELL_WIDTH: f32 = 96.0;
const DRAG_WIDTH: f32 = 92.0;
const DRAG_HEIGHT: f32 = 78.0;

#[derive(Debug, Clone, Copy)]
struct TableGeometry {
    cell_width: f32,
    cell_height: f32,
    cell_gap: f32,
    table_width: f32,
    table_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DragState {
    atomic_number: u8,
    pointer: Point,
    positioned: bool,
}

#[derive(Debug, Default)]
pub struct State {
    selected: Option<u8>,
    hovered: Option<u8>,
    dragging: Option<DragState>,
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    HoverChanged(Option<u8>),
    DragStarted(u8),
    DragMoved(Point),
    DragEnded,
}

pub fn update(state: &mut State, message: Message) {
    match message {
        Message::HoverChanged(hovered) => state.hovered = hovered,
        Message::DragStarted(atomic_number) => {
            state.selected = Some(atomic_number);
            state.dragging = Some(DragState {
                atomic_number,
                pointer: Point::ORIGIN,
                positioned: false,
            });
        }
        Message::DragMoved(pointer) => {
            if let Some(dragging) = &mut state.dragging {
                dragging.pointer = pointer;
                dragging.positioned = true;
            }
        }
        Message::DragEnded => state.dragging = None,
    }
}

pub fn subscription(state: &State) -> Subscription<Message> {
    if state.dragging.is_none() {
        return Subscription::none();
    }

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
    let accent = category_color(element.category);
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

pub fn view(state: &State, compact: bool) -> Element<'_, Message> {
    let table = responsive(move |size| periodic_grid(state, size.width)).height(Length::Shrink);
    container(table)
        .style(theme::frame)
        .padding(Padding {
            top: if compact { spacing::XXS } else { spacing::XS },
            right: if compact { spacing::XXS } else { spacing::XS },
            bottom: 0.0,
            left: if compact { spacing::XXS } else { spacing::XS },
        })
        .width(Fill)
        .into()
}

fn periodic_grid(state: &State, available_width: f32) -> Element<'static, Message> {
    let geometry = table_geometry(available_width);
    let group_numbers = (1..=GROUPS).fold(row![].spacing(geometry.cell_gap), |groups, group| {
        groups.push(
            container(
                text(group.to_string())
                    .size(type_scale::MICRO)
                    .color(color::FAINT),
            )
            .center_x(geometry.cell_width)
            .height(Length::Fixed(18.0)),
        )
    });

    let mut grid = Grid::new()
        .columns(GROUPS)
        .spacing(geometry.cell_gap)
        .width(geometry.table_width)
        .height(geometry.table_height);

    for period in 1..=PERIODS {
        for group in 1..=GROUPS {
            let element = elements::SUPPORTED.iter().find(|element| {
                usize::from(element.period) == period && usize::from(element.group) == group
            });

            grid = grid.push(match element {
                Some(element) => element_tile(state, *element, geometry),
                None => empty_cell(geometry),
            });
        }
    }

    let table = column![group_numbers, grid]
        .spacing(spacing::XXS)
        .width(Length::Fixed(geometry.table_width));

    container(column![table, category_legend(geometry.cell_width < 44.0),].spacing(spacing::XXS))
        .style(theme::panel)
        .padding(Padding {
            top: if available_width < 720.0 {
                spacing::XXS
            } else {
                spacing::XS
            },
            right: spacing::XS,
            bottom: 0.0,
            left: spacing::XS,
        })
        .width(Fill)
        .into()
}

fn element_tile(
    state: &State,
    element: ElementSpec,
    geometry: TableGeometry,
) -> Element<'static, Message> {
    let dimmed = false;
    let selected = state.selected == Some(element.atomic_number);
    let dragging = state
        .dragging
        .is_some_and(|drag| drag.atomic_number == element.atomic_number);
    let hovered = state.hovered == Some(element.atomic_number);
    let foreground = if dimmed { color::FAINT } else { color::TEXT };
    let secondary = if dimmed { color::FAINT } else { color::MUTED };
    let dense = geometry.cell_width < 44.0;
    let content: Element<'static, Message> = if dense {
        column![
            text(element.atomic_number.to_string())
                .size(7)
                .color(secondary),
            text(element.symbol).size(13).color(foreground),
            text(element.atomic_mass).size(7).color(secondary),
        ]
        .spacing(0)
        .width(Fill)
        .into()
    } else {
        column![
            row![
                text(element.atomic_number.to_string())
                    .size(8)
                    .color(secondary),
                space().width(Fill),
                text(element.atomic_mass).size(8).color(secondary),
            ],
            row![
                text(element.symbol).size(17).color(foreground),
                space().width(Fill),
                text(element.name).size(8).color(secondary),
            ]
            .align_y(iced::Center),
        ]
        .spacing(0)
        .width(Fill)
        .into()
    };

    let tile = container(content)
        .padding(if dense {
            Padding::new(1.0)
        } else {
            Padding::new(2.0)
        })
        .width(Length::Fixed(geometry.cell_width))
        .height(Length::Fixed(geometry.cell_height))
        .style(move |_| {
            let emphasis = if dragging {
                TileEmphasis::Dragging
            } else if selected {
                TileEmphasis::Selected
            } else if hovered {
                TileEmphasis::Hovered
            } else {
                TileEmphasis::Idle
            };

            tile_style(element.category, dimmed, emphasis)
        });

    mouse_area(tile)
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
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba(
                color::CANVAS_RAISED.r,
                color::CANVAS_RAISED.g,
                color::CANVAS_RAISED.b,
                0.28,
            ))),
            border: Border {
                color: Color::from_rgba(color::LINE.r, color::LINE.g, color::LINE.b, 0.34),
                width: 1.0,
                radius: border::Radius::new(radius::CONTROL),
            },
            ..container::Style::default()
        })
        .width(Length::Fixed(geometry.cell_width))
        .height(Length::Fixed(geometry.cell_height))
        .into()
}

fn table_geometry(available_width: f32) -> TableGeometry {
    let cell_gap = if available_width < 900.0 { 2.0 } else { 6.0 };
    let gaps = TABLE_GAPS * cell_gap;
    let usable_width = (available_width - spacing::MD * 2.0).max(0.0);
    let cell_width = ((usable_width - gaps) / GROUPS_F32).clamp(MIN_CELL_WIDTH, MAX_CELL_WIDTH);
    let cell_height = (cell_width * 0.68).clamp(28.0, 46.0);
    let table_width = GROUPS_F32 * cell_width + gaps;
    let table_height = PERIODS_F32 * cell_height + 4.0 * cell_gap;

    TableGeometry {
        cell_width,
        cell_height,
        cell_gap,
        table_width,
        table_height,
    }
}

fn category_legend(dense: bool) -> Element<'static, Message> {
    let categories = [
        Category::AlkaliMetal,
        Category::AlkalineEarth,
        Category::TransitionMetal,
        Category::PostTransitionMetal,
        Category::Metalloid,
        Category::ReactiveNonmetal,
        Category::Halogen,
        Category::NobleGas,
    ];

    let legend_item = |category| {
        row![
            text("●")
                .size(type_scale::CAPTION)
                .color(category_color(category)),
            text(category.label())
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS)
    };

    if dense {
        let first = categories[..4]
            .iter()
            .copied()
            .fold(row![].spacing(spacing::SM), |row, category| {
                row.push(legend_item(category))
            });
        let second = categories[4..]
            .iter()
            .copied()
            .fold(row![].spacing(spacing::SM), |row, category| {
                row.push(legend_item(category))
            });
        column![first, second].spacing(spacing::XXS).into()
    } else {
        categories
            .into_iter()
            .fold(row![].spacing(spacing::SM), |legend, category| {
                legend.push(legend_item(category))
            })
            .into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TileEmphasis {
    Idle,
    Hovered,
    Selected,
    Dragging,
}

fn tile_style(category: Category, dimmed: bool, emphasis: TileEmphasis) -> container::Style {
    let accent = category_color(category);
    let is_hovered = emphasis == TileEmphasis::Hovered;
    let is_selected = emphasis == TileEmphasis::Selected;
    let is_dragging = emphasis == TileEmphasis::Dragging;

    let background = if dimmed {
        Color::from_rgba(
            color::CANVAS_RAISED.r,
            color::CANVAS_RAISED.g,
            color::CANVAS_RAISED.b,
            0.5,
        )
    } else if is_dragging {
        Color::from_rgba(accent.r, accent.g, accent.b, 0.24)
    } else if is_selected || is_hovered {
        Color::from_rgba(accent.r, accent.g, accent.b, 0.14)
    } else {
        color::SURFACE
    };

    let border_color = if dimmed {
        Color::from_rgba(color::LINE.r, color::LINE.g, color::LINE.b, 0.42)
    } else if is_selected || is_dragging || is_hovered {
        accent
    } else {
        color::LINE_STRONG
    };

    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(if dimmed { color::FAINT } else { color::TEXT }),
        border: Border {
            color: border_color,
            width: if is_selected || is_dragging { 2.0 } else { 1.0 },
            radius: border::Radius::new(radius::CONTROL),
        },
        shadow: if is_dragging {
            Shadow {
                color: Color::from_rgba(accent.r, accent.g, accent.b, 0.28),
                offset: Vector::new(0.0, 6.0),
                blur_radius: 16.0,
            }
        } else {
            Shadow::default()
        },
        ..container::Style::default()
    }
}

fn category_color(category: Category) -> Color {
    match category {
        Category::AlkaliMetal => Color::from_rgb(0.91, 0.49, 0.72),
        Category::AlkalineEarth => Color::from_rgb(0.95, 0.71, 0.35),
        Category::TransitionMetal => Color::from_rgb(0.56, 0.77, 1.0),
        Category::PostTransitionMetal => Color::from_rgb(0.54, 0.84, 0.86),
        Category::Metalloid => Color::from_rgb(0.72, 0.64, 0.96),
        Category::ReactiveNonmetal => Color::from_rgb(0.43, 0.84, 0.58),
        Category::Halogen => Color::from_rgb(0.50, 0.86, 0.75),
        Category::NobleGas => Color::from_rgb(0.43, 0.76, 0.94),
    }
}

fn drag_style(accent: Color) -> container::Style {
    container::Style::default()
        .background(Color::from_rgba(accent.r, accent.g, accent.b, 0.25))
        .border(Border {
            color: accent,
            width: 2.0,
            radius: border::Radius::new(radius::CONTROL),
        })
        .shadow(Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.48),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 22.0,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_selects_an_element_and_global_release_cancels_it() {
        let mut state = State::default();

        update(&mut state, Message::DragStarted(8));
        update(&mut state, Message::DragMoved(Point::new(120.0, 80.0)));
        assert_eq!(state.selected, Some(8));
        assert!(state.dragging.is_some());

        update(&mut state, Message::DragEnded);
        assert!(state.dragging.is_none());
    }

    #[test]
    fn table_geometry_always_fits_the_available_width_without_scrolling() {
        let compact = table_geometry(620.0);
        let desktop = table_geometry(1_360.0);
        let wide = table_geometry(1_900.0);

        assert!(compact.table_width <= 620.0);
        assert!(desktop.table_width <= 1_360.0);
        assert!(wide.table_width <= 1_900.0);
        assert!(desktop.cell_width > compact.cell_width);
        assert!(wide.cell_width > desktop.cell_width);
        assert!(wide.cell_width <= MAX_CELL_WIDTH);
    }
}
