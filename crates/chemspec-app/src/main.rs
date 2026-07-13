//! `ChemSpec` application shell (`U-101`).
//!
//! Shows the six product regions — request, workflow, source, validation,
//! sources, and simulation — populated from the canonical silver-chloride
//! fixture as static content. No parsing, validation, or agent work happens
//! here yet; every region renders placeholder state that the real subsystems
//! will replace.

mod vessel;

use iced::widget::{button, canvas, column, container, row, rule, scrollable, text, text_input};
use iced::{Center, Element, Fill, FillPortion, Font, Theme};

use vessel::Vessel;

const CANONICAL_SOURCE: &str = include_str!("../../../fixtures/silver-chloride.chems");
const CANONICAL_REQUEST: &str = "What happens if I mix 50 mL of 0.100 M silver nitrate \
     with 50 mL of 0.100 M sodium chloride?";
const SIMULATION_DISCLOSURE: &str = "Explanatory particle model. Quantities and reaction \
     relationships are validated; particle scale, motion, and elapsed time are illustrative.";

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("ChemSpec")
        .theme(App::theme)
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    RequestChanged(String),
    RequestSubmitted,
}

struct App {
    request: String,
    source: String,
    vessel: Vessel,
}

impl Default for App {
    fn default() -> Self {
        Self {
            request: CANONICAL_REQUEST.to_owned(),
            source: CANONICAL_SOURCE.to_owned(),
            vessel: Vessel::new(),
        }
    }
}

impl App {
    fn update(&mut self, message: Message) {
        match message {
            Message::RequestChanged(request) => self.request = request,
            // Agent orchestration arrives with `A-101`/`U-105`; until then
            // submitting keeps showing the canonical offline fixture.
            Message::RequestSubmitted => {}
        }
    }

    fn theme(_: &Self) -> Theme {
        Theme::TokyoNight
    }

    fn view(&self) -> Element<'_, Message> {
        let left = column![
            self.request_panel(),
            Self::workflow_panel(),
            Self::sources_panel(),
        ]
        .spacing(10)
        .width(FillPortion(3));

        let middle = column![self.source_panel(), Self::validation_panel()]
            .spacing(10)
            .width(FillPortion(4));

        let right = column![self.simulation_panel()]
            .spacing(10)
            .width(FillPortion(5));

        let header = row![
            text("ChemSpec").size(26),
            text("virtual chemistry laboratory").size(14),
        ]
        .spacing(12)
        .align_y(Center);

        column![header, row![left, middle, right].spacing(10).height(Fill)]
            .spacing(10)
            .padding(12)
            .into()
    }

    fn request_panel(&self) -> Element<'_, Message> {
        panel(
            "Request",
            column![
                text_input("Ask what happens when substances mix...", &self.request)
                    .on_input(Message::RequestChanged)
                    .on_submit(Message::RequestSubmitted),
                row![
                    button("Ask").on_press(Message::RequestSubmitted),
                    text("Provider: not configured").size(12),
                ]
                .spacing(10)
                .align_y(Center),
            ]
            .spacing(10)
            .into(),
        )
    }

    fn workflow_panel() -> Element<'static, Message> {
        let steps = [
            ("✓", "Identified the requested substances"),
            ("✓", "Researched aqueous behaviour"),
            ("✓", "Predicted the reaction"),
            ("✓", "Wrote .chems"),
            ("✓", "Validated"),
        ];

        let list = steps
            .into_iter()
            .fold(column![].spacing(4), |col, (mark, label)| {
                col.push(row![text(mark).width(18), text(label).size(13)].spacing(4))
            });

        panel(
            "Workflow",
            column![
                list,
                text("Offline fixture — agent workflow lands in Phase 3").size(11)
            ]
            .spacing(8)
            .into(),
        )
    }

    fn sources_panel() -> Element<'static, Message> {
        let card = |title: &'static str, claim: &'static str| {
            container(column![text(title).size(13), text(claim).size(11)].spacing(3))
                .style(container::bordered_box)
                .padding(8)
                .width(Fill)
        };

        panel(
            "Sources",
            column![
                card(
                    "OpenStax Chemistry 2e §4.2",
                    "Silver chloride is insoluble in water at 25 degC.",
                ),
                card(
                    "ChemSpec.Aqueous@1 catalogue",
                    "AgNO3, NaCl, and NaNO3 are soluble strong electrolytes.",
                ),
            ]
            .spacing(6)
            .into(),
        )
    }

    fn source_panel(&self) -> Element<'_, Message> {
        panel(
            "Experiment source — silver-chloride.chems",
            scrollable(text(&self.source).size(12).font(Font::MONOSPACE))
                .height(Fill)
                .into(),
        )
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

        let list = checks.into_iter().fold(column![].spacing(3), |col, check| {
            col.push(row![text("✓").width(18), text(check).size(13)].spacing(4))
        });

        let assumptions = [
            "Aqueous solutions",
            "25 degC",
            "1 atm",
            "Idealized complete dissociation",
        ]
        .into_iter()
        .fold(column![].spacing(3), |col, item| {
            col.push(row![text("•").width(18), text(item).size(13)].spacing(4))
        });

        panel(
            "Validation",
            column![
                text("Validated with assumptions").size(15),
                list,
                rule::horizontal(1),
                text("Assumptions").size(13),
                assumptions,
            ]
            .spacing(8)
            .into(),
        )
    }

    fn simulation_panel(&self) -> Element<'_, Message> {
        panel(
            "Simulation",
            column![
                text("Canonical initial state — dissolved ions after mixing").size(12),
                canvas(&self.vessel).width(Fill).height(Fill),
                Vessel::legend(),
                text(SIMULATION_DISCLOSURE).size(11),
            ]
            .spacing(10)
            .into(),
        )
    }
}

fn panel<'a>(title: &'a str, content: Element<'a, Message>) -> Element<'a, Message> {
    container(column![text(title).size(16), content].spacing(8))
        .style(container::bordered_box)
        .padding(12)
        .width(Fill)
        .into()
}
