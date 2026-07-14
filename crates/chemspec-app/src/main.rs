//! `ChemSpec` application shell and reaction-builder entry (`U-101`, `U-106`–`U-111`).
//!
//! Opens on the Stage 1 element library and preserves the six validated-record
//! regions—request, workflow, source, validation, sources, and simulation—using
//! the canonical silver-chloride fixture. No parsing, validation, or agent work
//! happens here yet; presentation does not confer chemistry meaning.

mod composition_catalogue;
mod elements;
mod particle_visualization;
mod periodic_table;
mod reaction_candidate_catalogue;
mod reaction_sequence;
mod reaction_workspace;
mod theme;
mod vessel;

use iced::widget::{
    button, canvas, column, container, responsive, row, rule, scrollable, space, stack, text,
    text_input,
};
use iced::{Center, Element, Fill, FillPortion, Font, Length, Size, Subscription, Theme};

use theme::{breakpoint, color, space as spacing, type_scale};
use vessel::Vessel;

const CANONICAL_SOURCE: &str = include_str!("../../../fixtures/silver-chloride.chems");
const CANONICAL_REQUEST: &str = "What happens if I mix 50 mL of 0.100 M silver nitrate \
     with 50 mL of 0.100 M sodium chloride?";
const CANONICAL_EQUATION: &str = "AgNO₃ + NaCl  →  AgCl↓ + NaNO₃";
const SIMULATION_DISCLOSURE: &str = "Explanatory particle model. Quantities and reaction \
     relationships are validated; particle scale, motion, and elapsed time are illustrative.";

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("ChemSpec — reaction builder")
        .subscription(App::subscription)
        .theme(App::theme)
        .window(iced::window::Settings {
            size: Size::new(1_440.0, 900.0),
            min_size: Some(Size::new(560.0, 760.0)),
            position: iced::window::Position::Centered,
            ..iced::window::Settings::default()
        })
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Builder,
    ValidatedRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Overview,
    Source,
    Validation,
    Evidence,
}

impl Section {
    const ALL: [Self; 4] = [
        Self::Overview,
        Self::Source,
        Self::Validation,
        Self::Evidence,
    ];

    const fn label(self, compact: bool) -> &'static str {
        match (self, compact) {
            (Self::Overview, true) => "Run",
            (Self::Overview, false) => "Overview",
            (Self::Source, true) => ".chems",
            (Self::Source, false) => "Source",
            (Self::Validation, true) => "Checks",
            (Self::Validation, false) => "Validation",
            (Self::Evidence, true) => "Sources",
            (Self::Evidence, false) => "Evidence",
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    ScreenSelected(Screen),
    PeriodicTable(periodic_table::Message),
    ReactionWorkspace(reaction_workspace::Message),
    RequestChanged(String),
    RequestSubmitted,
    SectionSelected(Section),
}

struct App {
    screen: Screen,
    periodic_table: periodic_table::State,
    reaction_workspace: reaction_workspace::State,
    request: String,
    source: String,
    section: Section,
    vessel: Vessel,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Builder,
            periodic_table: periodic_table::State::default(),
            reaction_workspace: reaction_workspace::State::default(),
            request: CANONICAL_REQUEST.to_owned(),
            source: CANONICAL_SOURCE.to_owned(),
            section: Section::Overview,
            vessel: Vessel::new(),
        }
    }
}

impl App {
    fn update(&mut self, message: Message) {
        match message {
            Message::ScreenSelected(screen) => self.screen = screen,
            Message::PeriodicTable(message) => {
                periodic_table::update(&mut self.periodic_table, message);
            }
            Message::ReactionWorkspace(message) => {
                reaction_workspace::update(&mut self.reaction_workspace, message);
            }
            Message::RequestChanged(request) => self.request = request,
            // Agent orchestration arrives with `A-101`/`U-105`; until then
            // submitting keeps showing the canonical offline fixture.
            Message::RequestSubmitted => {}
            Message::SectionSelected(section) => self.section = section,
        }
    }

    fn theme(_: &Self) -> Theme {
        theme::app_theme()
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.screen == Screen::Builder {
            Subscription::batch([
                periodic_table::subscription(&self.periodic_table).map(Message::PeriodicTable),
                reaction_workspace::subscription(&self.reaction_workspace)
                    .map(Message::ReactionWorkspace),
            ])
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::Builder => responsive(|size| self.builder_view(size)).into(),
            Screen::ValidatedRecord => responsive(|size| self.responsive_view(size)).into(),
        }
    }

    fn builder_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let outer_padding = if compact { spacing::XS } else { spacing::SM };
        let sequence_active = reaction_workspace::sequence_active(&self.reaction_workspace);

        let library =
            periodic_table::view(&self.periodic_table, compact).map(Message::PeriodicTable);
        let workspace = reaction_workspace::view(
            &self.reaction_workspace,
            periodic_table::dragging_atomic_number(&self.periodic_table),
            compact,
        )
        .map(Message::ReactionWorkspace);

        let stages: Element<'_, Message> = if sequence_active {
            container(workspace).width(Fill).height(Fill).into()
        } else {
            column![workspace, library]
                .spacing(spacing::XS)
                .width(Fill)
                .height(Fill)
                .into()
        };

        let content = column![
            Self::builder_context_bar(compact, sequence_active),
            stages,
            Self::builder_status_bar(compact, sequence_active),
        ]
        .spacing(spacing::XS)
        .height(Fill);

        let application = container(content)
            .style(theme::app_background)
            .padding(outer_padding)
            .width(Fill)
            .height(Fill);
        let drag_overlay =
            periodic_table::drag_overlay(&self.periodic_table, size).map(Message::PeriodicTable);

        stack![application, drag_overlay]
            .width(Fill)
            .height(Fill)
            .clip(false)
            .into()
    }

    fn responsive_view(&self, size: Size) -> Element<'_, Message> {
        if size.width >= breakpoint::DESKTOP {
            self.desktop_view()
        } else if size.width >= breakpoint::MOBILE {
            self.tablet_view()
        } else {
            self.mobile_view()
        }
    }

    fn desktop_view(&self) -> Element<'_, Message> {
        let workspace = row![
            container(self.simulation_panel(Fill))
                .width(FillPortion(7))
                .height(Fill),
            container(self.inspector(false, Fill))
                .width(FillPortion(5))
                .height(Fill),
        ]
        .spacing(spacing::MD)
        .height(Fill);

        let content = column![
            Self::context_bar(false),
            self.request_panel(false),
            workspace,
            Self::status_bar(false),
        ]
        .spacing(spacing::SM)
        .height(Fill);

        Self::application_frame(content.into(), spacing::XL)
    }

    fn tablet_view(&self) -> Element<'_, Message> {
        let content = column![
            Self::context_bar(false),
            self.request_panel(false),
            self.simulation_panel(Length::Fixed(480.0)),
            self.inspector(false, Length::Fixed(590.0)),
            Self::status_bar(false),
        ]
        .spacing(spacing::SM);

        Self::scrollable_frame(content.into(), spacing::MD)
    }

    fn mobile_view(&self) -> Element<'_, Message> {
        let content = column![
            Self::context_bar(true),
            self.request_panel(true),
            self.simulation_panel(Length::Fixed(420.0)),
            self.inspector(true, Length::Fixed(650.0)),
            Self::status_bar(true),
        ]
        .spacing(spacing::SM);

        Self::scrollable_frame(content.into(), spacing::SM)
    }

    fn application_frame(
        content: Element<'_, Message>,
        outer_padding: f32,
    ) -> Element<'_, Message> {
        container(
            container(content)
                .style(theme::frame)
                .padding(spacing::MD)
                .width(Fill)
                .height(Fill),
        )
        .style(theme::app_background)
        .padding(outer_padding)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn scrollable_frame(content: Element<'_, Message>, outer_padding: f32) -> Element<'_, Message> {
        let page = container(content)
            .style(theme::frame)
            .padding(spacing::SM)
            .width(Fill);

        container(scrollable(page).width(Fill))
            .style(theme::app_background)
            .padding(outer_padding)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn context_bar(compact: bool) -> Element<'static, Message> {
        let brand = row![
            container(text("CS").size(type_scale::CAPTION).color(color::ACCENT))
                .style(theme::accent_tint)
                .center_x(34)
                .center_y(30),
            column![
                text(if compact {
                    "CHEMSPEC"
                } else {
                    "CHEMSPEC  /  VALIDATED REACTION WORKSPACE"
                })
                .size(type_scale::MICRO)
                .color(color::TEXT_SOFT),
                text("Virtual chemistry laboratory")
                    .size(type_scale::CAPTION)
                    .color(color::MUTED),
            ]
            .spacing(spacing::XXS),
        ]
        .spacing(spacing::SM)
        .align_y(Center);

        let context = if compact {
            text("OFFLINE FIXTURE")
                .size(type_scale::MICRO)
                .color(color::MUTED)
        } else {
            text(CANONICAL_EQUATION)
                .size(type_scale::BODY)
                .color(color::TEXT_SOFT)
        };

        let builder = button(text(if compact {
            "Build"
        } else {
            "← Reaction builder"
        }))
        .on_press(Message::ScreenSelected(Screen::Builder))
        .padding([spacing::XS, spacing::SM])
        .style(theme::secondary_button);

        container(
            row![brand, space().width(Fill), context, builder]
                .spacing(spacing::SM)
                .align_y(Center),
        )
        .style(theme::chrome)
        .padding([spacing::XS, spacing::SM])
        .width(Fill)
        .into()
    }

    fn builder_context_bar(compact: bool, sequence_active: bool) -> Element<'static, Message> {
        let brand = row![
            container(text("CS").size(type_scale::CAPTION).color(color::ACCENT))
                .style(theme::accent_tint)
                .center_x(34)
                .center_y(30),
            column![
                text("CHEMSPEC  /  REACTION BUILDER")
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
                text(if compact {
                    if sequence_active {
                        "Stage 5 · Animate"
                    } else {
                        "Stage 4 · Start"
                    }
                } else {
                    "Elements  →  Workspace  →  Visualise  →  Start  •  Animate  →  Result"
                })
                .size(type_scale::CAPTION)
                .color(color::MUTED),
            ]
            .spacing(spacing::XXS),
        ]
        .spacing(spacing::SM)
        .align_y(Center);

        let record = button(text(if compact {
            "Record"
        } else {
            "Validated record  →"
        }))
        .on_press(Message::ScreenSelected(Screen::ValidatedRecord))
        .padding([spacing::XS, spacing::SM])
        .style(theme::secondary_button);

        container(row![brand, space().width(Fill), record].align_y(Center))
            .style(theme::chrome)
            .padding([spacing::XS, spacing::SM])
            .width(Fill)
            .into()
    }

    fn builder_status_bar(compact: bool, sequence_active: bool) -> Element<'static, Message> {
        container(
            row![
                text(if sequence_active {
                    "STAGE 5 · 2D PREVIEW"
                } else {
                    "STAGE 5 READY FOR REVIEW"
                })
                .size(type_scale::MICRO)
                .color(color::SUCCESS),
                space().width(Fill),
                text(if compact {
                    "NEXT · 3D VIEW"
                } else {
                    "NEXT · 3D LAB VISUALISATION · LOCKED UNTIL APPROVAL"
                })
                .size(type_scale::MICRO)
                .color(color::MUTED),
            ]
            .align_y(Center),
        )
        .style(theme::chrome)
        .padding([spacing::XS, spacing::SM])
        .width(Fill)
        .into()
    }

    fn request_panel(&self, compact: bool) -> Element<'_, Message> {
        let heading = column![
            text("ASK THE LAB")
                .size(type_scale::MICRO)
                .color(color::ACCENT),
            text("Explore a reaction")
                .size(if compact {
                    type_scale::TITLE
                } else {
                    type_scale::DISPLAY
                })
                .color(color::TEXT),
            text("Describe the substances and quantities in ordinary language.")
                .size(type_scale::BODY)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS);

        let input = text_input("Ask what happens when substances mix…", &self.request)
            .on_input(Message::RequestChanged)
            .on_submit(Message::RequestSubmitted)
            .padding([spacing::SM, spacing::MD])
            .size(type_scale::BODY_LARGE)
            .style(theme::request_input)
            .width(Fill);

        let submit = button(
            row![text("Run fixture"), text("→").size(type_scale::BODY_LARGE)]
                .spacing(spacing::XS)
                .align_y(Center),
        )
        .on_press(Message::RequestSubmitted)
        .padding([spacing::SM, spacing::MD])
        .style(theme::primary_button);

        let controls: Element<'_, Message> = if compact {
            column![input, submit.width(Fill)]
                .spacing(spacing::XS)
                .into()
        } else {
            row![input, submit]
                .spacing(spacing::XS)
                .align_y(Center)
                .into()
        };

        let provider = row![
            text("●").size(type_scale::CAPTION).color(color::WARNING),
            text("Provider not configured · canonical offline fixture")
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .spacing(spacing::XS)
        .align_y(Center);

        container(column![heading, controls, provider].spacing(spacing::SM))
            .style(theme::panel)
            .padding(if compact { spacing::MD } else { spacing::LG })
            .width(Fill)
            .into()
    }

    fn simulation_panel(&self, height: Length) -> Element<'_, Message> {
        let title = column![
            text("REACTION STAGE")
                .size(type_scale::MICRO)
                .color(color::ACCENT),
            text("Silver chloride formation")
                .size(type_scale::TITLE)
                .color(color::TEXT),
            text("Initial state · dissolved ions after mixing")
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS);

        let status = container(
            row![
                text("●").size(type_scale::CAPTION).color(color::SUCCESS),
                text("VALIDATED WITH ASSUMPTIONS")
                    .size(type_scale::MICRO)
                    .color(color::TEXT_SOFT),
            ]
            .spacing(spacing::XS)
            .align_y(Center),
        )
        .style(theme::success_tint)
        .padding([spacing::XS, spacing::SM]);

        let stage = container(canvas(&self.vessel).width(Fill).height(Fill))
            .style(theme::inset)
            .padding(spacing::XS)
            .width(Fill)
            .height(Fill);

        container(
            column![
                row![title, space().width(Fill), status].align_y(Center),
                stage,
                Vessel::legend(),
                text(SIMULATION_DISCLOSURE)
                    .size(type_scale::CAPTION)
                    .color(color::MUTED),
            ]
            .spacing(spacing::SM),
        )
        .style(theme::panel)
        .padding(spacing::MD)
        .width(Fill)
        .height(height)
        .into()
    }

    fn inspector(&self, compact: bool, height: Length) -> Element<'_, Message> {
        let navigation =
            Section::ALL
                .into_iter()
                .fold(row![].spacing(spacing::XXS), |navigation, section| {
                    let selected = section == self.section;
                    navigation.push(
                        button(text(section.label(compact)).size(type_scale::CAPTION))
                            .on_press(Message::SectionSelected(section))
                            .padding([spacing::XS, spacing::SM])
                            .style(move |_, status| theme::navigation_button(selected, status)),
                    )
                });

        let content = match self.section {
            Section::Overview => Self::overview_panel(),
            Section::Source => self.source_panel(),
            Section::Validation => Self::validation_panel(),
            Section::Evidence => Self::sources_panel(),
        };

        container(column![navigation, content].spacing(spacing::SM))
            .style(theme::panel)
            .padding(spacing::SM)
            .width(Fill)
            .height(height)
            .into()
    }

    fn overview_panel() -> Element<'static, Message> {
        let workflow = Self::workflow_panel();

        let validation_summary = Self::summary_card(
            "VALIDATION",
            "6 checks passed",
            "Assumptions remain visible and inspectable.",
            Section::Validation,
        );

        let source_summary = Self::summary_card(
            "EXPERIMENT SOURCE",
            "silver-chloride.chems",
            "Human-readable source · chems 1",
            Section::Source,
        );

        let evidence_summary = Self::summary_card(
            "EVIDENCE",
            "2 linked sources",
            "Claims remain separate from trusted catalogue facts.",
            Section::Evidence,
        );

        scrollable(
            column![
                workflow,
                validation_summary,
                source_summary,
                evidence_summary,
            ]
            .spacing(spacing::XS),
        )
        .height(Fill)
        .into()
    }

    fn workflow_panel() -> Element<'static, Message> {
        let steps = [
            ("01", "Identified the requested substances"),
            ("02", "Researched aqueous behaviour"),
            ("03", "Predicted the reaction"),
            ("04", "Wrote .chems"),
            ("05", "Validated"),
        ];

        let list =
            steps
                .into_iter()
                .fold(column![].spacing(spacing::XS), |list, (number, label)| {
                    let marker =
                        container(text(number).size(type_scale::MICRO).color(color::SUCCESS))
                            .style(theme::success_tint)
                            .center_x(30)
                            .center_y(30);

                    list.push(
                        row![
                            marker,
                            column![
                                text(label).size(type_scale::BODY).color(color::TEXT_SOFT),
                                text("Complete").size(type_scale::MICRO).color(color::MUTED),
                            ]
                            .spacing(spacing::XXS),
                        ]
                        .spacing(spacing::SM)
                        .align_y(Center),
                    )
                });

        container(
            column![
                row![
                    column![
                        text("WORKFLOW")
                            .size(type_scale::MICRO)
                            .color(color::ACCENT),
                        text("Research to trusted result")
                            .size(type_scale::BODY_LARGE)
                            .color(color::TEXT),
                    ]
                    .spacing(spacing::XXS),
                    space().width(Fill),
                    text("5 / 5")
                        .size(type_scale::CAPTION)
                        .color(color::SUCCESS),
                ]
                .align_y(Center),
                rule::horizontal(1).style(|current| iced::widget::rule::Style {
                    color: color::LINE,
                    ..iced::widget::rule::default(current)
                }),
                list,
                text("Offline fixture · live agent progress arrives in Phase 3")
                    .size(type_scale::CAPTION)
                    .color(color::MUTED),
            ]
            .spacing(spacing::SM),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .into()
    }

    fn summary_card(
        eyebrow: &'static str,
        title: &'static str,
        detail: &'static str,
        section: Section,
    ) -> Element<'static, Message> {
        let content = column![
            text(eyebrow).size(type_scale::MICRO).color(color::ACCENT),
            text(title).size(type_scale::BODY_LARGE).color(color::TEXT),
            text(detail).size(type_scale::CAPTION).color(color::MUTED),
        ]
        .spacing(spacing::XXS)
        .width(Fill);

        container(
            row![
                content,
                button(text("Open  →").size(type_scale::CAPTION))
                    .on_press(Message::SectionSelected(section))
                    .padding([spacing::XS, spacing::SM])
                    .style(theme::secondary_button),
            ]
            .spacing(spacing::SM)
            .align_y(Center),
        )
        .style(theme::raised)
        .padding(spacing::SM)
        .width(Fill)
        .into()
    }

    fn source_panel(&self) -> Element<'_, Message> {
        let source = scrollable(
            container(
                text(&self.source)
                    .size(type_scale::CAPTION)
                    .font(Font::MONOSPACE)
                    .color(color::TEXT_SOFT),
            )
            .padding(spacing::MD)
            .width(Fill),
        )
        .height(Fill);

        container(
            column![
                Self::panel_heading(
                    "EXPERIMENT SOURCE",
                    "silver-chloride.chems",
                    "Visible proposal · not trusted until validation",
                ),
                source,
            ]
            .spacing(spacing::SM),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn validation_panel() -> Element<'static, Message> {
        let checks = [
            "Syntax and types",
            "Known substances",
            "Atoms conserved",
            "Charge conserved",
            "Precipitation rule established",
            "Stoichiometry solved",
        ];

        let list = checks
            .into_iter()
            .fold(column![].spacing(spacing::XS), |list, check| {
                list.push(
                    row![
                        text("✓").size(type_scale::BODY).color(color::SUCCESS),
                        text(check).size(type_scale::BODY).color(color::TEXT_SOFT),
                        space().width(Fill),
                        text("PASS").size(type_scale::MICRO).color(color::MUTED),
                    ]
                    .spacing(spacing::XS)
                    .align_y(Center),
                )
            });

        let assumptions = [
            "Aqueous solutions",
            "25 degC",
            "1 atm",
            "Idealized complete dissociation",
        ]
        .into_iter()
        .fold(column![].spacing(spacing::XS), |list, item| {
            list.push(
                container(
                    row![
                        text("◆").size(type_scale::MICRO).color(color::WARNING),
                        text(item).size(type_scale::CAPTION).color(color::TEXT_SOFT),
                    ]
                    .spacing(spacing::XS)
                    .align_y(Center),
                )
                .style(theme::raised)
                .padding([spacing::XS, spacing::SM])
                .width(Fill),
            )
        });

        container(
            scrollable(
                column![
                    Self::panel_heading(
                        "VALIDATION",
                        "Validated with assumptions",
                        "Deterministic checks on the current fixture",
                    ),
                    container(list)
                        .style(theme::raised)
                        .padding(spacing::MD)
                        .width(Fill),
                    text("ASSUMPTIONS")
                        .size(type_scale::MICRO)
                        .color(color::WARNING),
                    assumptions,
                ]
                .spacing(spacing::SM),
            )
            .height(Fill),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn sources_panel() -> Element<'static, Message> {
        let source_card =
            |index: &'static str, title: &'static str, kind: &'static str, claim: &'static str| {
                container(
                    column![
                        row![
                            text(index).size(type_scale::MICRO).color(color::ACCENT),
                            text(kind).size(type_scale::MICRO).color(color::MUTED),
                        ]
                        .spacing(spacing::XS),
                        text(title).size(type_scale::BODY_LARGE).color(color::TEXT),
                        text(claim).size(type_scale::BODY).color(color::TEXT_SOFT),
                    ]
                    .spacing(spacing::XS),
                )
                .style(theme::raised)
                .padding(spacing::MD)
                .width(Fill)
            };

        container(
            scrollable(
                column![
                    Self::panel_heading(
                        "EVIDENCE",
                        "Sources and catalogue claims",
                        "Provenance stays separate from .chems source",
                    ),
                    source_card(
                        "01",
                        "OpenStax Chemistry 2e §4.2",
                        "REFERENCE",
                        "Silver chloride is insoluble in water at 25 degC.",
                    ),
                    source_card(
                        "02",
                        "ChemSpec.Aqueous@1 catalogue",
                        "TRUSTED CATALOGUE",
                        "AgNO₃, NaCl, and NaNO₃ are soluble strong electrolytes.",
                    ),
                    container(
                        text(
                            "Evidence supports claims; it does not bypass deterministic validation."
                        )
                        .size(type_scale::CAPTION)
                        .color(color::MUTED),
                    )
                    .style(theme::accent_tint)
                    .padding(spacing::SM)
                    .width(Fill),
                ]
                .spacing(spacing::SM),
            )
            .height(Fill),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn panel_heading(
        eyebrow: &'static str,
        title: &'static str,
        subtitle: &'static str,
    ) -> Element<'static, Message> {
        column![
            text(eyebrow).size(type_scale::MICRO).color(color::ACCENT),
            text(title).size(type_scale::TITLE).color(color::TEXT),
            text(subtitle).size(type_scale::CAPTION).color(color::MUTED),
        ]
        .spacing(spacing::XXS)
        .into()
    }

    fn status_bar(compact: bool) -> Element<'static, Message> {
        let right = if compact {
            "STATIC SHELL"
        } else {
            "U-101  ·  STATIC SHELL  ·  NO PROVIDER USAGE"
        };

        container(
            row![
                text("EXPLANATORY MODEL")
                    .size(type_scale::MICRO)
                    .color(color::MUTED),
                space().width(Fill),
                text(right).size(type_scale::MICRO).color(color::FAINT),
            ]
            .align_y(Center),
        )
        .style(theme::chrome)
        .padding([spacing::XS, spacing::SM])
        .width(Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_edit_preserves_the_static_fixture() {
        let mut app = App::default();
        let source = app.source.clone();

        app.update(Message::RequestChanged("A different question".to_owned()));
        app.update(Message::RequestSubmitted);

        assert_eq!(app.request, "A different question");
        assert_eq!(app.source, source);
    }

    #[test]
    fn every_inspector_region_is_reachable() {
        let mut app = App::default();

        for section in Section::ALL {
            app.update(Message::SectionSelected(section));
            assert_eq!(app.section, section);
        }
    }

    #[test]
    fn all_responsive_compositions_build() {
        let app = App::default();

        for size in [
            Size::new(560.0, 620.0),
            Size::new(900.0, 800.0),
            Size::new(1_440.0, 900.0),
        ] {
            let _ = app.builder_view(size);
            let _ = app.responsive_view(size);
        }
    }

    #[test]
    fn periodic_drag_can_drop_directly_into_workspace() {
        let mut app = App::default();

        app.update(Message::PeriodicTable(
            periodic_table::Message::DragStarted(8),
        ));
        let dragged = periodic_table::dragging_atomic_number(&app.periodic_table)
            .expect("periodic drag should remain active outside the tile");
        app.update(Message::ReactionWorkspace(
            reaction_workspace::Message::PointerMoved(iced::Point::new(0.4, 0.5)),
        ));
        app.update(Message::ReactionWorkspace(
            reaction_workspace::Message::LibraryElementDropped(dragged),
        ));
        app.update(Message::PeriodicTable(periodic_table::Message::DragEnded));

        assert_eq!(
            reaction_workspace::placed_atom_count(&app.reaction_workspace),
            1
        );
    }
}
