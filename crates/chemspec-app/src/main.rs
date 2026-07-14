//! `ChemSpec` application shell and reaction-builder entry (`U-101`, `U-106`–`U-112`).
//!
//! Opens on the Stage 1 element library and preserves the six validated-record
//! regions—request, workflow, source, validation, sources, and simulation.
//! Chemistry is supplied only through the host-pinned language/kernel boundary.

mod chemistry;
mod composition_catalogue;
mod elements;
mod particle_visualization;
mod periodic_table;
mod reactant_composer;
mod reaction_sequence;
mod reaction_workspace;
mod theme;

use iced::widget::{
    button, canvas, column, container, responsive, row, rule, scrollable, space, stack, text,
    text_editor, text_input,
};
use iced::{Center, Element, Fill, FillPortion, Font, Length, Size, Subscription, Theme};

use theme::{breakpoint, color, space as spacing, type_scale};

const CANONICAL_SOURCE: &str = chemistry::SOURCE;
const CANONICAL_REQUEST: &str = chemistry::REQUEST;
const CANONICAL_EQUATION: &str = chemistry::EQUATION;
const SIMULATION_DISCLOSURE: &str = chemistry::DISCLOSURE;

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

fn codex_available() -> bool {
    std::process::Command::new("codex")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    ProviderSetup,
    Builder,
    ValidatedRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderChoice {
    CodexSubscription,
    ApiKey,
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
    ProviderSelected(ProviderChoice),
    ApiKeyChanged(String),
    ProviderContinue,
    PeriodicTable(periodic_table::Message),
    ReactantComposer(reactant_composer::Message),
    ReactionWorkspace(reaction_workspace::Message),
    RequestChanged(String),
    RequestSubmitted,
    SourceEdited(text_editor::Action),
    SourceRevalidate,
    SectionSelected(Section),
}

struct App {
    screen: Screen,
    codex_available: bool,
    provider: Option<ProviderChoice>,
    api_key: String,
    periodic_table: periodic_table::State,
    reactant_composer: reactant_composer::State,
    reaction_workspace: reaction_workspace::State,
    request: String,
    source: text_editor::Content,
    validated_frames: Option<chem_kernel::SimulationFrames>,
    validation_error: Option<String>,
    source_stale: bool,
    section: Section,
}

impl Default for App {
    fn default() -> Self {
        let (validated_frames, validation_error) = match chemistry::canonical_run() {
            Ok(run) => (Some(run.frames().clone()), None),
            Err(error) => (None, Some(error.to_owned())),
        };
        Self {
            screen: Screen::ProviderSetup,
            codex_available: codex_available(),
            provider: None,
            api_key: String::new(),
            periodic_table: periodic_table::State::default(),
            reactant_composer: reactant_composer::State::default(),
            reaction_workspace: reaction_workspace::State::default(),
            request: CANONICAL_REQUEST.to_owned(),
            source: text_editor::Content::with_text(CANONICAL_SOURCE),
            validated_frames,
            validation_error,
            source_stale: false,
            section: Section::Overview,
        }
    }
}

impl App {
    fn update(&mut self, message: Message) {
        match message {
            Message::ScreenSelected(screen) => self.screen = screen,
            Message::ProviderSelected(provider) => self.provider = Some(provider),
            Message::ApiKeyChanged(api_key) => self.api_key = api_key,
            Message::ProviderContinue => {
                let ready = match self.provider {
                    Some(ProviderChoice::CodexSubscription) => self.codex_available,
                    Some(ProviderChoice::ApiKey) => !self.api_key.trim().is_empty(),
                    None => false,
                };
                if ready {
                    self.screen = Screen::Builder;
                }
            }
            Message::PeriodicTable(message) => {
                periodic_table::update(&mut self.periodic_table, message);
                if !reaction_workspace::sequence_active(&self.reaction_workspace)
                    && let periodic_table::Message::Activated(atomic_number) = message
                {
                    reactant_composer::update(
                        &mut self.reactant_composer,
                        reactant_composer::Message::AddElement(atomic_number),
                    );
                }
            }
            Message::ReactantComposer(message) => {
                if matches!(message, reactant_composer::Message::StartReactionRequested)
                    && reactant_composer::can_start_reaction(&self.reactant_composer)
                {
                    let (first, second) = reactant_composer::reactants(&self.reactant_composer);
                    reaction_workspace::load_reactants(&mut self.reaction_workspace, first, second);
                    reaction_workspace::update(
                        &mut self.reaction_workspace,
                        reaction_workspace::Message::StartReaction,
                    );
                } else {
                    reactant_composer::update(&mut self.reactant_composer, message);
                }
            }
            Message::ReactionWorkspace(message) => {
                reaction_workspace::update(&mut self.reaction_workspace, message);
            }
            Message::RequestChanged(request) => self.request = request,
            // The offline fixture crosses the same trusted language/kernel
            // boundary that live provider output must cross later.
            Message::RequestSubmitted => {
                self.revalidate_source();
                if self.validated_frames.is_some() {
                    self.screen = Screen::ValidatedRecord;
                }
            }
            Message::SourceEdited(action) => {
                let is_edit = action.is_edit();
                self.source.perform(action);
                if is_edit {
                    self.source_stale = true;
                    self.validated_frames = None;
                    self.validation_error = None;
                }
            }
            Message::SourceRevalidate => self.revalidate_source(),
            Message::SectionSelected(section) => self.section = section,
        }
    }

    fn revalidate_source(&mut self) {
        match chemistry::validate_source(&self.source.text()) {
            Ok(frames) => {
                self.validated_frames = Some(frames);
                self.validation_error = None;
                self.source_stale = false;
            }
            Err(error) => {
                self.validated_frames = None;
                self.validation_error = Some(error);
                self.source_stale = false;
            }
        }
    }

    fn theme(_: &Self) -> Theme {
        theme::app_theme()
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.screen == Screen::Builder {
            let composer = if reaction_workspace::sequence_active(&self.reaction_workspace) {
                Subscription::none()
            } else {
                reactant_composer::subscription(&self.reactant_composer)
                    .map(Message::ReactantComposer)
            };
            Subscription::batch([
                periodic_table::subscription(&self.periodic_table).map(Message::PeriodicTable),
                composer,
                reaction_workspace::subscription(&self.reaction_workspace)
                    .map(Message::ReactionWorkspace),
            ])
        } else {
            Subscription::none()
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match self.screen {
            Screen::ProviderSetup => responsive(|size| self.provider_setup_view(size)).into(),
            Screen::Builder => responsive(|size| self.builder_view(size)).into(),
            Screen::ValidatedRecord => responsive(|size| self.responsive_view(size)).into(),
        }
    }

    fn provider_setup_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let codex_selected = self.provider == Some(ProviderChoice::CodexSubscription);
        let api_selected = self.provider == Some(ProviderChoice::ApiKey);

        let codex = button(
            column![
                text("Use Codex subscription")
                    .size(type_scale::BODY_LARGE)
                    .color(color::TEXT),
                text(if self.codex_available {
                    "codex binary detected · recommended"
                } else {
                    "codex binary not found on PATH"
                })
                .size(type_scale::CAPTION)
                .color(if self.codex_available {
                    color::SUCCESS
                } else {
                    color::WARNING
                }),
            ]
            .spacing(spacing::XXS),
        )
        .on_press_maybe(
            self.codex_available
                .then_some(Message::ProviderSelected(ProviderChoice::CodexSubscription)),
        )
        .padding(spacing::MD)
        .width(Fill)
        .style(move |_, status| theme::navigation_button(codex_selected, status));

        let api = button(
            column![
                text("Use API key")
                    .size(type_scale::BODY_LARGE)
                    .color(color::TEXT),
                text("No Codex installation required · kept in memory")
                    .size(type_scale::CAPTION)
                    .color(color::MUTED),
            ]
            .spacing(spacing::XXS),
        )
        .on_press(Message::ProviderSelected(ProviderChoice::ApiKey))
        .padding(spacing::MD)
        .width(Fill)
        .style(move |_, status| theme::navigation_button(api_selected, status));

        let choices: Element<'_, Message> = if compact {
            column![codex, api].spacing(spacing::SM).into()
        } else {
            row![codex, api].spacing(spacing::SM).into()
        };
        let api_key: Element<'_, Message> = if api_selected {
            text_input("OpenAI API key", &self.api_key)
                .on_input(Message::ApiKeyChanged)
                .secure(true)
                .padding(spacing::SM)
                .into()
        } else {
            space().height(Length::Shrink).into()
        };
        let ready = (codex_selected && self.codex_available)
            || (api_selected && !self.api_key.trim().is_empty());

        let content = container(
            column![
                text("CHEMSPEC  /  PROVIDER")
                    .size(type_scale::MICRO)
                    .color(color::ACCENT),
                text("How should ChemSpec research reactions?")
                    .size(if compact {
                        type_scale::TITLE
                    } else {
                        type_scale::DISPLAY
                    })
                    .color(color::TEXT),
                text("Choose Codex subscription for the primary experience or an API key for the dependency-free mode.")
                    .size(type_scale::BODY)
                    .color(color::MUTED),
                choices,
                api_key,
                row![
                    text("The canonical offline fixture remains available after setup.")
                        .size(type_scale::CAPTION)
                        .color(color::MUTED),
                    space().width(Fill),
                    button(text("Continue  →"))
                        .on_press_maybe(ready.then_some(Message::ProviderContinue))
                        .padding([spacing::SM, spacing::MD])
                        .style(theme::primary_button),
                ]
                .align_y(Center),
            ]
            .spacing(spacing::MD),
        )
        .style(theme::frame)
        .padding(if compact { spacing::MD } else { spacing::XL })
        .width(Length::Fill)
        .max_width(900.0);

        container(content)
            .style(theme::app_background)
            .center(Fill)
            .into()
    }

    fn builder_view(&self, size: Size) -> Element<'_, Message> {
        let compact = size.width < breakpoint::MOBILE;
        let outer_padding = if compact { spacing::XS } else { spacing::SM };
        let sequence_active = reaction_workspace::sequence_active(&self.reaction_workspace);

        let library =
            periodic_table::view(&self.periodic_table, compact).map(Message::PeriodicTable);
        let composer = reactant_composer::view(
            &self.reactant_composer,
            periodic_table::dragging_atomic_number(&self.periodic_table),
            compact,
        )
        .map(Message::ReactantComposer);
        let sequence: Element<'_, Message> =
            reaction_workspace::view(&self.reaction_workspace, None, compact)
                .map(Message::ReactionWorkspace);

        let stages: Element<'_, Message> = if sequence_active {
            container(sequence).width(Fill).height(Fill).into()
        } else {
            column![composer, library]
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
            text("AI-REVIEWED FIXTURE")
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
                        "Stage 1 · Build"
                    }
                } else {
                    "Elements  →  Build reactants  →  Animate  →  Result"
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
                    "STAGE 1 · REACTANT COMPOSER"
                })
                .size(type_scale::MICRO)
                .color(color::SUCCESS),
                space().width(Fill),
                text(if !sequence_active {
                    if compact {
                        "NEXT · REACTION PREVIEW"
                    } else {
                        "NEXT · 2D REACTION PREVIEW · LOCKED UNTIL A SUPPORTED PAIR IS SET"
                    }
                } else if compact {
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
            text(match self.provider {
                Some(ProviderChoice::CodexSubscription) => {
                    "Codex subscription selected · trusted fixture active"
                }
                Some(ProviderChoice::ApiKey) => "API key selected · trusted fixture active",
                None => "Provider not configured · trusted canonical fixture",
            })
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
            text(chemistry::NAME)
                .size(type_scale::TITLE)
                .color(color::TEXT),
            text("Final validated state · products assigned")
                .size(type_scale::CAPTION)
                .color(color::MUTED),
        ]
        .spacing(spacing::XXS);

        let valid = self.validated_frames.is_some();
        let status = container(
            row![
                text("●").size(type_scale::CAPTION).color(color::SUCCESS),
                text(if valid {
                    "VALIDATED · AI-REVIEWED CATALOGUE"
                } else if self.source_stale {
                    "STALE · REVALIDATION REQUIRED"
                } else {
                    "INVALID OR UNSUPPORTED"
                })
                .size(type_scale::MICRO)
                .color(color::TEXT_SOFT),
            ]
            .spacing(spacing::XS)
            .align_y(Center),
        )
        .style(if valid {
            theme::success_tint
        } else {
            theme::accent_tint
        })
        .padding([spacing::XS, spacing::SM]);

        let stage: Element<'_, Message> = if let Some(frames) = &self.validated_frames {
            let final_frame = frames.frames().len().saturating_sub(1);
            container(
                canvas(reaction_sequence::ReactionSequenceDiagram::new(
                    frames,
                    final_frame,
                ))
                .width(Fill)
                .height(Fill),
            )
            .style(theme::inset)
            .padding(spacing::XS)
            .width(Fill)
            .height(Fill)
            .into()
        } else {
            container(
                text(
                    self.validation_error
                        .as_deref()
                        .unwrap_or("Source changed. Revalidate before playback."),
                )
                .size(type_scale::BODY)
                .color(color::WARNING),
            )
            .style(theme::inset)
            .padding(spacing::MD)
            .center(Fill)
            .into()
        };

        container(
            column![
                row![title, space().width(Fill), status].align_y(Center),
                stage,
                row![
                    text("━ covalent").color(color::ACCENT),
                    text("◯ ionic association").color(color::SUCCESS),
                    text("◉ metallic domain").color(color::WARNING),
                ]
                .spacing(spacing::MD),
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
            Section::Validation => self.validation_panel(),
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
            "Trusted structural derivation",
            "Exact source, catalogue, evidence, and frame identities are bound.",
            Section::Validation,
        );

        let source_summary = Self::summary_card(
            "EXPERIMENT SOURCE",
            "lithium-water.chems",
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
                text("Offline trusted fixture · live provider events remain to be connected")
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
        let source = text_editor(&self.source)
            .on_action(Message::SourceEdited)
            .font(Font::MONOSPACE)
            .size(type_scale::CAPTION)
            .padding(spacing::MD)
            .height(Fill);
        let status = if self.source_stale {
            "Edited · downstream validation and frames invalidated"
        } else if self.validated_frames.is_some() {
            "Current · source identity matches the trusted frame artifact"
        } else {
            "Validation failed · inspect the diagnostic below"
        };
        let diagnostic = self.validation_error.as_deref().unwrap_or(status);

        container(
            column![
                Self::panel_heading(
                    "EXPERIMENT SOURCE",
                    chemistry::SOURCE_NAME,
                    "Parsed source · trusted only after expansion and kernel validation",
                ),
                source,
                row![
                    text(diagnostic).size(type_scale::CAPTION).color(
                        if self.validated_frames.is_some() {
                            color::SUCCESS
                        } else {
                            color::WARNING
                        }
                    ),
                    space().width(Fill),
                    button(text("Revalidate"))
                        .on_press(Message::SourceRevalidate)
                        .padding([spacing::XS, spacing::SM])
                        .style(theme::primary_button),
                ]
                .align_y(Center),
            ]
            .spacing(spacing::SM),
        )
        .style(theme::inset)
        .padding(spacing::MD)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn validation_panel(&self) -> Element<'_, Message> {
        let checks = [
            "Complete .chems 1 parse",
            "Host-pinned catalogue and AI attestation",
            "Total atom mapping",
            "Atom and charge conservation",
            "Electron and valence invariants",
            "Product graph and frame projection",
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
            "Representative educational outcome",
            "Explanatory sequence, not mechanism",
            "AI-reviewed catalogue trust decision",
            "Not laboratory or safety guidance",
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

        let identity = self.validated_frames.as_ref().map_or_else(
            || "No current validated identity".to_owned(),
            |frames| {
                format!(
                    "frame digest {}",
                    frames
                        .digest()
                        .map_or_else(|_| "unavailable".to_owned(), |digest| digest.to_string())
                )
            },
        );

        container(
            scrollable(
                column![
                    Self::panel_heading(
                        "VALIDATION",
                        "Valid · exact trusted capability",
                        "Deterministic checks over the current source and catalogue",
                    ),
                    text(identity).size(type_scale::CAPTION).color(color::MUTED),
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
                        "OpenStax Chemistry 2e",
                        "REFERENCE",
                        "Supports the structural and representative lithium-water premises.",
                    ),
                    source_card(
                        "02",
                        "IUPAC Gold Book",
                        "REFERENCE",
                        "Supports ionic, metallic, and bonding terminology used by the model.",
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
            "TRUSTED FIXTURE"
        } else {
            "TRUSTED KERNEL FRAMES  ·  AI-REVIEWED CATALOGUE  ·  OFFLINE"
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
    fn api_key_provider_requires_an_in_memory_key_before_continuing() {
        let mut app = App::default();
        assert_eq!(app.screen, Screen::ProviderSetup);
        app.update(Message::ProviderSelected(ProviderChoice::ApiKey));
        app.update(Message::ProviderContinue);
        assert_eq!(app.screen, Screen::ProviderSetup);
        app.update(Message::ApiKeyChanged("test-only-key".to_owned()));
        app.update(Message::ProviderContinue);
        assert_eq!(app.screen, Screen::Builder);
    }

    #[test]
    fn request_edit_preserves_the_canonical_source() {
        let mut app = App::default();
        let source = app.source.text();

        app.update(Message::RequestChanged("A different question".to_owned()));
        app.update(Message::RequestSubmitted);

        assert_eq!(app.request, "A different question");
        assert_eq!(app.source.text(), source);
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

    #[test]
    fn stage_one_supported_drafts_launch_the_sequence_without_a_workspace_screen() {
        let mut app = App::default();

        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            3,
        )));
        app.update(Message::ReactantComposer(
            reactant_composer::Message::Activate(reactant_composer::ActiveReactant::Second),
        ));
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            1,
        )));
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            1,
        )));
        app.update(Message::PeriodicTable(periodic_table::Message::Activated(
            8,
        )));

        assert_eq!(reactant_composer::reactants(&app.reactant_composer).0, &[3]);
        assert_eq!(
            reactant_composer::reactants(&app.reactant_composer).1,
            &[1, 1, 8]
        );

        app.update(Message::ReactantComposer(
            reactant_composer::Message::StartReactionRequested,
        ));

        assert!(reaction_workspace::sequence_active(&app.reaction_workspace));
        assert_eq!(
            reaction_workspace::placed_atom_count(&app.reaction_workspace),
            4
        );

        app.update(Message::ReactionWorkspace(
            reaction_workspace::Message::WorkspaceReturned,
        ));
        assert!(!reaction_workspace::sequence_active(
            &app.reaction_workspace
        ));
    }
}
