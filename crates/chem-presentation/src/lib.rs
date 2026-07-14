#![forbid(unsafe_code)]

//! Deterministic, renderer-independent animation planning.
//!
//! This crate owns explanatory pacing and macroscopic scene composition. It
//! accepts only a trusted [`ValidatedStructuralReaction`]; it never parses
//! `.chems`, resolves catalogue data, or infers chemistry.

use std::collections::BTreeSet;
use std::fmt;

use chem_catalogue::{
    AssetProfile, AtomState, CameraBehaviour, CameraCue, EffectIntensity, ObservationClaim,
    PresentationEffect, PresentationObject, ReviewedEquation, StoichiometricTerm,
    StructuralOperation,
};
use chem_domain::ContentDigest;
use chem_engine::{ObservationStage, StructuralFrame, ValidatedStructuralReaction};
use serde::{Deserialize, Serialize};

pub const VIRTUAL_ONLY_DISCLOSURE: &str = "Virtual educational model—not a laboratory procedure. Do not reproduce without qualified supervision and an appropriate risk assessment.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EducationalSceneKind {
    Introduction,
    ReactantSetup,
    Equation,
    StructuralChange,
    ExplanationPause,
    ObservationConnection,
    Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExplanationLabelKind {
    ConceptExplanation,
    StructuralChangeExplanation,
    ObservationExplanation,
    EquationExplanation,
    ImportantResult,
    SummaryExplanation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplanationLabel {
    pub kind: ExplanationLabelKind,
    pub text: String,
    pub target_atoms: Vec<String>,
    pub connector: bool,
}

/// Concise deterministic copy displayed beside trusted structural content.
///
/// The planner populates this label from the validated `.chems` meaning and
/// reviewed rule. Renderers control placement and styling only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextLabel {
    pub kind: ExplanationLabelKind,
    pub title: String,
    pub text: String,
    pub target_atoms: Vec<String>,
    pub connector: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeciesRole {
    Reactant,
    Product,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EducationalCue {
    IntroduceSpecies {
        role: SpeciesRole,
        species: Vec<String>,
    },
    ShowEquation {
        equation: ReviewedEquation,
    },
    EstablishFrame {
        frame: ContentDigest,
    },
    ApplyOperation {
        operation: StructuralOperation,
        before: ContentDigest,
        after: ContentDigest,
        affected_atoms: Vec<String>,
    },
    ShowObservation {
        observation_id: String,
        frame: ContentDigest,
    },
    ShowExplanation {
        label: ExplanationLabel,
    },
    ShowContext {
        label: ContextLabel,
    },
    PreserveDisclosure {
        event_model: String,
        sequence_model: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EducationalScene {
    pub kind: EducationalSceneKind,
    pub start_frame: ContentDigest,
    pub end_frame: ContentDigest,
    pub duration_ms: u32,
    pub cues: Vec<EducationalCue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EducationalPlan {
    pub id: ContentDigest,
    pub reaction: ContentDigest,
    pub scenes: Vec<EducationalScene>,
    pub event_model: String,
    pub sequence_model: String,
}

/// A deterministic position on an [`EducationalPlan`] timeline.
///
/// The elapsed value is always local to the selected scene. Positions returned
/// by [`EducationalPlan::locate`] are clamped to the scene duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelinePosition {
    pub scene_index: usize,
    pub scene_elapsed_ms: u32,
}

impl EducationalPlan {
    /// Returns the total duration of every scene on the educational timeline.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.scenes.iter().fold(0_u64, |duration, scene| {
            duration.saturating_add(u64::from(scene.duration_ms))
        })
    }

    /// Locates an absolute elapsed time on the educational timeline.
    ///
    /// Times beyond the end are clamped. An exact scene boundary belongs to
    /// the following non-zero-duration scene, while the end of the complete
    /// timeline belongs to the final scene. Zero-duration scenes are skipped
    /// at intermediate boundaries. If every scene has zero duration, the final
    /// scene is returned at zero elapsed time.
    #[must_use]
    pub fn locate(&self, elapsed_ms: u64) -> Option<TimelinePosition> {
        let final_index = self.scenes.len().checked_sub(1)?;
        let duration_ms = self.duration_ms();
        let elapsed_ms = elapsed_ms.min(duration_ms);

        if elapsed_ms == duration_ms {
            let scene = &self.scenes[final_index];
            return Some(TimelinePosition {
                scene_index: final_index,
                scene_elapsed_ms: scene.duration_ms,
            });
        }

        let mut remaining_ms = elapsed_ms;
        for (scene_index, scene) in self.scenes.iter().enumerate() {
            let scene_duration_ms = u64::from(scene.duration_ms);
            if scene_duration_ms == 0 {
                continue;
            }
            if remaining_ms < scene_duration_ms {
                return Some(TimelinePosition {
                    scene_index,
                    scene_elapsed_ms: u32::try_from(remaining_ms).unwrap_or(scene.duration_ms),
                });
            }
            remaining_ms -= scene_duration_ms;
        }

        Some(TimelinePosition {
            scene_index: final_index,
            scene_elapsed_ms: self.scenes[final_index].duration_ms,
        })
    }

    /// Converts a scene-local position back into absolute elapsed time.
    ///
    /// The local elapsed value is clamped to the selected scene duration.
    /// Returns `None` when the scene index does not exist.
    #[must_use]
    pub fn elapsed_at(&self, position: TimelinePosition) -> Option<u64> {
        let scene = self.scenes.get(position.scene_index)?;
        let elapsed_before = self
            .scenes
            .iter()
            .take(position.scene_index)
            .fold(0_u64, |duration, scene| {
                duration.saturating_add(u64::from(scene.duration_ms))
            });
        Some(
            elapsed_before
                .saturating_add(u64::from(position.scene_elapsed_ms.min(scene.duration_ms))),
        )
    }
}

/// Compiles validated structural meaning into a reusable teaching narrative.
/// Operation-specific drawing remains the renderer's concern.
///
/// # Errors
///
/// Returns an error when frames are absent, out of sequence, or inconsistent
/// with their embedded operations.
pub fn compile_educational_plan(
    reaction: &ValidatedStructuralReaction,
    frames: &[StructuralFrame],
) -> Result<EducationalPlan, PlanError> {
    let first = frames.first().ok_or(PlanError::MissingFrames)?;
    let last = frames.last().ok_or(PlanError::MissingFrames)?;
    if frames
        .iter()
        .enumerate()
        .any(|(index, frame)| usize::from(frame.ordinal) != index)
    {
        return Err(PlanError::InvalidFrameSequence);
    }

    let mut scenes = introductory_scenes(reaction, first);

    for pair in frames.windows(2) {
        let before = &pair[0];
        let after = &pair[1];
        let operation = after
            .active_operation
            .clone()
            .ok_or(PlanError::MissingOperation(after.ordinal))?;
        let narration = operation_narration(reaction, before, after, &operation)?;
        let duration_ms = operation_duration(&operation)
            .saturating_add(explanation_duration(&narration.explanation.text));
        scenes.push(EducationalScene {
            kind: EducationalSceneKind::StructuralChange,
            start_frame: before.id,
            end_frame: after.id,
            duration_ms,
            cues: vec![
                EducationalCue::EstablishFrame { frame: before.id },
                EducationalCue::ApplyOperation {
                    affected_atoms: affected_atoms(&operation),
                    operation: operation.clone(),
                    before: before.id,
                    after: after.id,
                },
                EducationalCue::ShowContext {
                    label: narration.context,
                },
                EducationalCue::ShowExplanation {
                    label: narration.explanation,
                },
            ],
        });
        let observations = after
            .observations
            .iter()
            .filter(|observation| observation.stage == ObservationStage::Active)
            .map(|observation| -> Result<EducationalScene, PlanError> {
                let text = observation_explanation(reaction, &observation.observation.claim)?;
                let target_atoms =
                    product_atoms(after, observation_species(&observation.observation.claim));
                let label = ExplanationLabel {
                    kind: ExplanationLabelKind::ObservationExplanation,
                    text,
                    target_atoms: target_atoms.clone(),
                    connector: !target_atoms.is_empty(),
                };
                Ok(EducationalScene {
                    kind: EducationalSceneKind::ObservationConnection,
                    start_frame: after.id,
                    end_frame: after.id,
                    duration_ms: explanation_duration(&label.text),
                    cues: vec![
                        EducationalCue::ShowObservation {
                            observation_id: observation.observation.id.clone(),
                            frame: after.id,
                        },
                        EducationalCue::ShowContext {
                            label: ContextLabel {
                                kind: ExplanationLabelKind::ObservationExplanation,
                                title: observation_title(&observation.observation.claim).to_owned(),
                                text: observation_context(
                                    reaction,
                                    &observation.observation.claim,
                                )?,
                                target_atoms,
                                connector: false,
                            },
                        },
                        EducationalCue::ShowExplanation { label },
                    ],
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        scenes.extend(observations);
    }

    scenes.push(summary_scene(reaction, last)?);

    let identity =
        serde_json::to_value((reaction.digest(), &scenes)).map_err(|_| PlanError::Serialization)?;
    Ok(EducationalPlan {
        id: ContentDigest::of_json(&identity).map_err(|_| PlanError::Serialization)?,
        reaction: reaction.digest(),
        scenes,
        event_model: first.event_model.clone(),
        sequence_model: first.sequence_model.clone(),
    })
}

#[derive(Debug)]
struct OperationNarration {
    context: ContextLabel,
    explanation: ExplanationLabel,
}

fn operation_narration(
    reaction: &ValidatedStructuralReaction,
    before: &StructuralFrame,
    after: &StructuralFrame,
    operation: &StructuralOperation,
) -> Result<OperationNarration, PlanError> {
    let target_atoms = affected_atoms(operation);
    let (title, context, explanation) = match operation {
        StructuralOperation::AssociateIonic { left, right } => {
            let left_label = charged_atom_label(after, before, left)?;
            let right_label = charged_atom_label(after, before, right)?;
            (
                "IONIC ASSOCIATION",
                format!("{left_label} attracts {right_label}"),
                format!(
                    "{left_label} and {right_label} now associate through electrostatic attraction."
                ),
            )
        }
        StructuralOperation::AssignProduct { product, .. } => {
            let formula = species_formula(reaction, product)?;
            (
                "PRODUCT ESTABLISHED",
                format!("{formula} unit established"),
                format!(
                    "These conserved atoms are assigned as one validated {formula} product unit."
                ),
            )
        }
        StructuralOperation::TransferMetallicElectron {
            donor_site,
            acceptor,
            count,
            ..
        } => {
            let donor = atom_symbol(after, before, donor_site)?;
            let acceptor_symbol = atom_symbol(after, before, acceptor)?;
            let acceptor_after = charged_atom_label(after, before, acceptor)?;
            let noun = if *count == 1 { "electron" } else { "electrons" };
            (
                "ELECTRON TRANSFER",
                format!("{donor} → {acceptor_symbol} · {count} {noun}"),
                format!(
                    "{donor} transfers {count} delocalised {noun} to {acceptor_symbol}; the validated acceptor state becomes {acceptor_after}."
                ),
            )
        }
        StructuralOperation::CleaveCovalent {
            left, right, order, ..
        } => {
            let left_symbol = atom_symbol(after, before, left)?;
            let right_symbol = atom_symbol(after, before, right)?;
            let order_name = bond_order_name(*order)?;
            (
                "BOND CLEAVAGE",
                format!("{left_symbol}–{right_symbol} {order_name} bond separates"),
                format!(
                    "The validated {order_name} {left_symbol}–{right_symbol} covalent bond cleaves, and both electron states update."
                ),
            )
        }
        StructuralOperation::FormCovalent {
            left, right, order, ..
        } => {
            let left_symbol = atom_symbol(after, before, left)?;
            let right_symbol = atom_symbol(after, before, right)?;
            let order_name = bond_order_name(*order)?;
            (
                "COVALENT BOND",
                format!("{left_symbol}–{right_symbol} {order_name} bond forms"),
                format!(
                    "{left_symbol} and {right_symbol} share electron density to form a validated {order_name} covalent bond."
                ),
            )
        }
    };
    Ok(OperationNarration {
        context: ContextLabel {
            kind: ExplanationLabelKind::StructuralChangeExplanation,
            title: title.to_owned(),
            text: context,
            target_atoms: target_atoms.clone(),
            connector: true,
        },
        explanation: ExplanationLabel {
            kind: ExplanationLabelKind::StructuralChangeExplanation,
            text: explanation,
            target_atoms,
            connector: true,
        },
    })
}

fn observation_explanation(
    reaction: &ValidatedStructuralReaction,
    claim: &ObservationClaim,
) -> Result<String, PlanError> {
    Ok(match claim {
        ObservationClaim::ProductForms { species } => {
            let formula = species_formula(reaction, species)?;
            format!("{formula} is now established as a validated product.")
        }
        ObservationClaim::ProductHasColour { species, colour } => {
            let formula = species_formula(reaction, species)?;
            format!("{formula} is observed as {colour}.")
        }
        ObservationClaim::GasEvolves { species } => {
            let formula = species_formula(reaction, species)?;
            format!("{formula} gas evolution is now observable.")
        }
        ObservationClaim::ReactantConsumed { species } => {
            let formula = species_formula(reaction, species)?;
            format!("{formula} is consumed as the validated sequence progresses.")
        }
    })
}

fn observation_context(
    reaction: &ValidatedStructuralReaction,
    claim: &ObservationClaim,
) -> Result<String, PlanError> {
    Ok(match claim {
        ObservationClaim::ProductForms { species } => {
            format!("{} forms", species_formula(reaction, species)?)
        }
        ObservationClaim::ProductHasColour { species, colour } => {
            format!("{} appears {colour}", species_formula(reaction, species)?)
        }
        ObservationClaim::GasEvolves { species } => {
            format!("{} gas evolves", species_formula(reaction, species)?)
        }
        ObservationClaim::ReactantConsumed { species } => {
            format!("{} is consumed", species_formula(reaction, species)?)
        }
    })
}

fn observation_title(claim: &ObservationClaim) -> &'static str {
    match claim {
        ObservationClaim::ProductForms { .. } => "PRODUCT FORMED",
        ObservationClaim::ProductHasColour { .. } => "COLOUR OBSERVATION",
        ObservationClaim::GasEvolves { .. } => "GAS EVOLUTION",
        ObservationClaim::ReactantConsumed { .. } => "REACTANT CONSUMED",
    }
}

fn atom_symbol<'a>(
    primary: &'a StructuralFrame,
    fallback: &'a StructuralFrame,
    atom_id: &str,
) -> Result<&'a str, PlanError> {
    atom_state(primary, fallback, atom_id)
        .map(|atom| atom.element.as_str())
        .ok_or(PlanError::UnknownNarrationAtom)
}

fn charged_atom_label(
    primary: &StructuralFrame,
    fallback: &StructuralFrame,
    atom_id: &str,
) -> Result<String, PlanError> {
    let atom = atom_state(primary, fallback, atom_id).ok_or(PlanError::UnknownNarrationAtom)?;
    Ok(format!(
        "{}{}",
        atom.element,
        formal_charge_suffix(atom.formal_charge)
    ))
}

fn atom_state<'a>(
    primary: &'a StructuralFrame,
    fallback: &'a StructuralFrame,
    atom_id: &str,
) -> Option<&'a AtomState> {
    primary
        .atoms
        .iter()
        .find(|atom| atom.id == atom_id)
        .or_else(|| fallback.atoms.iter().find(|atom| atom.id == atom_id))
}

fn formal_charge_suffix(charge: i8) -> String {
    match charge {
        0 => String::new(),
        1 => "⁺".to_owned(),
        -1 => "⁻".to_owned(),
        value => format!("({value:+})"),
    }
}

fn bond_order_name(order: u8) -> Result<&'static str, PlanError> {
    match order {
        1 => Ok("single"),
        2 => Ok("double"),
        3 => Ok("triple"),
        _ => Err(PlanError::UnsupportedNarrationBondOrder),
    }
}

fn species_formula<'a>(
    reaction: &'a ValidatedStructuralReaction,
    species: &str,
) -> Result<&'a str, PlanError> {
    reaction
        .equation()
        .reactants
        .iter()
        .chain(&reaction.equation().products)
        .find(|term| term.species == species)
        .map(|term| term.formula.as_str())
        .ok_or(PlanError::UnknownNarrationSpecies)
}

fn equation_side_text(terms: &[StoichiometricTerm]) -> String {
    terms
        .iter()
        .map(|term| {
            if term.coefficient == 1 {
                term.formula.clone()
            } else {
                format!("{} {}", term.coefficient, term.formula)
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn observation_species(claim: &ObservationClaim) -> &str {
    match claim {
        ObservationClaim::ProductForms { species }
        | ObservationClaim::ProductHasColour { species, .. }
        | ObservationClaim::GasEvolves { species }
        | ObservationClaim::ReactantConsumed { species } => species,
    }
}

fn product_atoms(frame: &StructuralFrame, species: &str) -> Vec<String> {
    let mut atoms = frame
        .product_memberships
        .iter()
        .filter(|membership| membership.product == species)
        .flat_map(|membership| membership.atoms.iter().cloned())
        .collect::<Vec<_>>();
    atoms.sort();
    atoms.dedup();
    atoms
}

fn explanation_duration(text: &str) -> u32 {
    let words = u32::try_from(text.split_whitespace().count()).unwrap_or(u32::MAX);
    (2_500_u32.saturating_add(words.saturating_mul(200))).clamp(3_600, 6_400)
}

fn introductory_scenes(
    reaction: &ValidatedStructuralReaction,
    first: &StructuralFrame,
) -> Vec<EducationalScene> {
    let mut reactant_cues = vec![
        EducationalCue::IntroduceSpecies {
            role: SpeciesRole::Reactant,
            species: reaction.reactants().to_vec(),
        },
        EducationalCue::EstablishFrame { frame: first.id },
        EducationalCue::ShowContext {
            label: ContextLabel {
                kind: ExplanationLabelKind::ConceptExplanation,
                title: "VALIDATED REACTANTS".to_owned(),
                text: equation_side_text(&reaction.equation().reactants),
                target_atoms: first.atoms.iter().map(|atom| atom.id.clone()).collect(),
                connector: false,
            },
        },
    ];
    reactant_cues.extend(first.metallic_domains.iter().map(|domain| {
        let noun = if domain.delocalized_electrons == 1 {
            "electron"
        } else {
            "electrons"
        };
        let site_noun = if domain.sites.len() == 1 {
            "site"
        } else {
            "sites"
        };
        EducationalCue::ShowContext {
            label: ContextLabel {
                kind: ExplanationLabelKind::ConceptExplanation,
                title: "METALLIC DOMAIN".to_owned(),
                text: format!(
                    "{} delocalised {noun} across {} {site_noun}",
                    domain.delocalized_electrons,
                    domain.sites.len()
                ),
                target_atoms: domain.sites.clone(),
                connector: true,
            },
        }
    }));

    vec![
        EducationalScene {
            kind: EducationalSceneKind::Introduction,
            start_frame: first.id,
            end_frame: first.id,
            duration_ms: 3_000,
            cues: vec![EducationalCue::PreserveDisclosure {
                event_model: first.event_model.clone(),
                sequence_model: first.sequence_model.clone(),
            }],
        },
        EducationalScene {
            kind: EducationalSceneKind::ReactantSetup,
            start_frame: first.id,
            end_frame: first.id,
            duration_ms: 4_000,
            cues: reactant_cues,
        },
        EducationalScene {
            kind: EducationalSceneKind::Equation,
            start_frame: first.id,
            end_frame: first.id,
            duration_ms: 3_800,
            cues: vec![EducationalCue::ShowEquation {
                equation: reaction.equation().clone(),
            }],
        },
    ]
}

fn summary_scene(
    reaction: &ValidatedStructuralReaction,
    last: &StructuralFrame,
) -> Result<EducationalScene, PlanError> {
    let mut cues = vec![
        EducationalCue::IntroduceSpecies {
            role: SpeciesRole::Product,
            species: reaction.products().to_vec(),
        },
        EducationalCue::ShowEquation {
            equation: reaction.equation().clone(),
        },
        EducationalCue::EstablishFrame { frame: last.id },
        EducationalCue::ShowContext {
            label: ContextLabel {
                kind: ExplanationLabelKind::ImportantResult,
                title: "VALIDATED PRODUCTS".to_owned(),
                text: equation_side_text(&reaction.equation().products),
                target_atoms: last.atoms.iter().map(|atom| atom.id.clone()).collect(),
                connector: false,
            },
        },
        EducationalCue::ShowExplanation {
            label: ExplanationLabel {
                kind: ExplanationLabelKind::SummaryExplanation,
                text: format!(
                    "The validated products {} and their reviewed observations are now established.",
                    equation_side_text(&reaction.equation().products)
                ),
                target_atoms: last.atoms.iter().map(|atom| atom.id.clone()).collect(),
                connector: false,
            },
        },
    ];
    cues.extend(
        reaction
            .observations()
            .iter()
            .map(|observation| -> Result<EducationalCue, PlanError> {
                let target_atoms = product_atoms(last, observation_species(&observation.claim));
                Ok(EducationalCue::ShowContext {
                    label: ContextLabel {
                        kind: ExplanationLabelKind::ObservationExplanation,
                        title: observation_title(&observation.claim).to_owned(),
                        text: observation_context(reaction, &observation.claim)?,
                        connector: !target_atoms.is_empty(),
                        target_atoms,
                    },
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    );

    Ok(EducationalScene {
        kind: EducationalSceneKind::Summary,
        start_frame: last.id,
        end_frame: last.id,
        duration_ms: 4_800,
        cues,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroscopicAnnotation {
    pub start_ordinal: u16,
    pub end_ordinal: u16,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealWorldBeat {
    pub start_ordinal: u16,
    pub end_ordinal: u16,
    pub duration_ms: u32,
    pub camera: CameraCue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealWorldTimeline {
    pub beats: Vec<RealWorldBeat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RealWorldPosition {
    pub beat_index: usize,
    pub ordinal: u16,
    pub ordinal_progress: f32,
    pub beat_progress: f32,
}

impl RealWorldTimeline {
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.beats.iter().fold(0_u64, |duration, beat| {
            duration.saturating_add(u64::from(beat.duration_ms))
        })
    }

    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn locate(&self, elapsed_ms: u64) -> Option<RealWorldPosition> {
        let final_index = self.beats.len().checked_sub(1)?;
        let duration_ms = self.duration_ms();
        let elapsed_ms = elapsed_ms.min(duration_ms);
        if elapsed_ms == duration_ms {
            let beat = &self.beats[final_index];
            return Some(RealWorldPosition {
                beat_index: final_index,
                ordinal: beat.end_ordinal,
                ordinal_progress: 1.0,
                beat_progress: 1.0,
            });
        }

        let mut remaining_ms = elapsed_ms;
        for (beat_index, beat) in self.beats.iter().enumerate() {
            let beat_duration = u64::from(beat.duration_ms);
            if remaining_ms >= beat_duration {
                remaining_ms -= beat_duration;
                continue;
            }
            let beat_progress = if beat.duration_ms == 0 {
                1.0
            } else {
                (remaining_ms as f32 / beat.duration_ms as f32).clamp(0.0, 1.0)
            };
            let ordinal_count = beat
                .end_ordinal
                .saturating_sub(beat.start_ordinal)
                .saturating_add(1);
            let scaled = beat_progress * f32::from(ordinal_count);
            let offset = scaled.floor() as u16;
            let ordinal = beat
                .start_ordinal
                .saturating_add(offset.min(ordinal_count.saturating_sub(1)));
            let ordinal_progress = if offset >= ordinal_count {
                1.0
            } else {
                scaled.fract()
            };
            return Some(RealWorldPosition {
                beat_index,
                ordinal,
                ordinal_progress,
                beat_progress,
            });
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenePlan {
    pub id: ContentDigest,
    pub reaction: ContentDigest,
    pub profile_id: String,
    pub environment: AssetProfile,
    pub objects: Vec<PresentationObject>,
    pub effects: Vec<PresentationEffect>,
    pub camera: Vec<CameraCue>,
    pub equation: ReviewedEquation,
    pub annotations: Vec<MacroscopicAnnotation>,
    pub timeline: RealWorldTimeline,
    pub disclosure: String,
    pub virtual_only_disclosure: String,
}

/// Compiles reviewed macroscopic metadata into a renderer-independent scene.
///
/// # Errors
///
/// Returns an error if the profile references an observation not present in
/// the trusted reaction. Catalogue loading should already reject this; the
/// planner keeps the boundary explicit for checked deserialization paths.
pub fn compile_real_world_plan(
    reaction: &ValidatedStructuralReaction,
) -> Result<ScenePlan, PlanError> {
    let profile = reaction.presentation();
    let observations = reaction
        .observations()
        .iter()
        .map(|observation| observation.id.as_str())
        .collect::<BTreeSet<_>>();
    if profile
        .effects
        .iter()
        .any(|effect| !observations.contains(effect.trigger_observation.as_str()))
    {
        return Err(PlanError::UnknownObservation);
    }
    let timeline = compile_real_world_timeline(reaction);
    let annotations = compile_macroscopic_annotations(reaction, &timeline)?;
    let identity = serde_json::to_value((
        reaction.digest(),
        profile,
        reaction.equation(),
        &annotations,
        &timeline,
    ))
    .map_err(|_| PlanError::Serialization)?;
    Ok(ScenePlan {
        id: ContentDigest::of_json(&identity).map_err(|_| PlanError::Serialization)?,
        reaction: reaction.digest(),
        profile_id: profile.id.clone(),
        environment: profile.environment,
        objects: profile.objects.clone(),
        effects: profile.effects.clone(),
        camera: profile.camera.clone(),
        equation: reaction.equation().clone(),
        annotations,
        timeline,
        disclosure: profile.disclosure.clone(),
        virtual_only_disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    })
}

fn compile_real_world_timeline(reaction: &ValidatedStructuralReaction) -> RealWorldTimeline {
    let profile = reaction.presentation();
    let final_ordinal = profile
        .objects
        .iter()
        .map(|object| object.visible_from_ordinal)
        .chain(profile.effects.iter().map(|effect| effect.end_ordinal))
        .chain(profile.camera.iter().map(|cue| cue.end_ordinal))
        .chain(
            reaction
                .observations()
                .iter()
                .map(|observation| observation.trigger_ordinal),
        )
        .max()
        .unwrap_or(0);
    let mut boundaries = BTreeSet::from([0, final_ordinal.saturating_add(1)]);
    for object in &profile.objects {
        boundaries.insert(object.visible_from_ordinal);
    }
    for effect in &profile.effects {
        boundaries.insert(effect.start_ordinal);
        boundaries.insert(effect.end_ordinal.saturating_add(1));
    }
    for cue in &profile.camera {
        boundaries.insert(cue.start_ordinal);
        boundaries.insert(cue.end_ordinal.saturating_add(1));
    }
    for observation in reaction.observations() {
        boundaries.insert(observation.trigger_ordinal);
    }
    let boundaries = boundaries
        .into_iter()
        .filter(|boundary| *boundary <= final_ordinal.saturating_add(1))
        .collect::<Vec<_>>();
    let beats = boundaries
        .windows(2)
        .filter_map(|window| {
            let start_ordinal = window[0];
            let end_ordinal = window[1].saturating_sub(1);
            (start_ordinal <= end_ordinal).then(|| {
                let observation_starts = reaction
                    .observations()
                    .iter()
                    .any(|observation| observation.trigger_ordinal == start_ordinal);
                let active_intensity = profile
                    .effects
                    .iter()
                    .filter(|effect| {
                        effect.start_ordinal <= start_ordinal && start_ordinal <= effect.end_ordinal
                    })
                    .map(|effect| effect.intensity)
                    .max_by_key(|intensity| match intensity {
                        EffectIntensity::Subtle => 0,
                        EffectIntensity::Moderate => 1,
                        EffectIntensity::Strong => 2,
                    });
                let duration_ms = real_world_beat_duration(
                    start_ordinal,
                    final_ordinal,
                    observation_starts,
                    active_intensity,
                );
                let selected_camera = profile
                    .camera
                    .iter()
                    .filter(|cue| {
                        cue.start_ordinal <= start_ordinal && start_ordinal <= cue.end_ordinal
                    })
                    .min_by_key(|cue| cue.end_ordinal.saturating_sub(cue.start_ordinal))
                    .or_else(|| {
                        profile
                            .camera
                            .iter()
                            .filter(|cue| cue.start_ordinal <= start_ordinal)
                            .max_by_key(|cue| cue.start_ordinal)
                    });
                let behaviour = selected_camera
                    .map_or(CameraBehaviour::WideEstablishingShot, |cue| cue.behaviour);
                RealWorldBeat {
                    start_ordinal,
                    end_ordinal,
                    duration_ms,
                    camera: CameraCue {
                        behaviour,
                        start_ordinal,
                        end_ordinal,
                    },
                }
            })
        })
        .collect();
    RealWorldTimeline { beats }
}

const fn real_world_beat_duration(
    start_ordinal: u16,
    final_ordinal: u16,
    observation_starts: bool,
    active_intensity: Option<EffectIntensity>,
) -> u32 {
    if start_ordinal == final_ordinal {
        5_600
    } else if observation_starts {
        5_400
    } else if let Some(intensity) = active_intensity {
        match intensity {
            EffectIntensity::Subtle => 5_600,
            EffectIntensity::Moderate => 6_400,
            EffectIntensity::Strong => 7_200,
        }
    } else if start_ordinal == 0 {
        4_200
    } else {
        4_400
    }
}

fn compile_macroscopic_annotations(
    reaction: &ValidatedStructuralReaction,
    timeline: &RealWorldTimeline,
) -> Result<Vec<MacroscopicAnnotation>, PlanError> {
    let final_ordinal = timeline.beats.last().map_or(0, |beat| beat.end_ordinal);
    let first_observation = reaction
        .observations()
        .iter()
        .map(|observation| observation.trigger_ordinal)
        .min()
        .unwrap_or(final_ordinal);
    let mut annotations = vec![MacroscopicAnnotation {
        start_ordinal: 0,
        end_ordinal: first_observation.saturating_sub(1),
        title: "INITIAL STATE".to_owned(),
        text: equation_side_text(&reaction.equation().reactants),
    }];
    let mut observations = reaction.observations().iter().collect::<Vec<_>>();
    observations.sort_by_key(|observation| observation.trigger_ordinal);
    for (index, observation) in observations.iter().enumerate() {
        let next = observations
            .get(index + 1)
            .map_or(final_ordinal, |next| next.trigger_ordinal.saturating_sub(1));
        annotations.push(MacroscopicAnnotation {
            start_ordinal: observation.trigger_ordinal,
            end_ordinal: next.max(observation.trigger_ordinal),
            title: observation_title(&observation.claim).to_owned(),
            text: observation_context(reaction, &observation.claim)?,
        });
    }
    annotations.push(MacroscopicAnnotation {
        start_ordinal: final_ordinal,
        end_ordinal: final_ordinal,
        title: "VALIDATED OUTCOME".to_owned(),
        text: equation_side_text(&reaction.equation().products),
    });
    Ok(annotations)
}

fn operation_duration(operation: &StructuralOperation) -> u32 {
    match operation {
        StructuralOperation::AssociateIonic { .. } => 4_000,
        StructuralOperation::AssignProduct { .. } => 3_200,
        StructuralOperation::TransferMetallicElectron { .. } => 5_000,
        StructuralOperation::CleaveCovalent { .. } | StructuralOperation::FormCovalent { .. } => {
            4_500
        }
    }
}

fn affected_atoms(operation: &StructuralOperation) -> Vec<String> {
    let mut atoms = match operation {
        StructuralOperation::AssociateIonic { left, right } => vec![left.clone(), right.clone()],
        StructuralOperation::AssignProduct { atoms, .. } => atoms.clone(),
        StructuralOperation::TransferMetallicElectron {
            donor_site,
            acceptor,
            ..
        } => vec![donor_site.clone(), acceptor.clone()],
        StructuralOperation::CleaveCovalent { left, right, .. }
        | StructuralOperation::FormCovalent { left, right, .. } => {
            vec![left.clone(), right.clone()]
        }
    };
    atoms.sort();
    atoms.dedup();
    atoms
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    MissingFrames,
    InvalidFrameSequence,
    MissingOperation(u16),
    UnknownNarrationAtom,
    UnknownNarrationSpecies,
    UnsupportedNarrationBondOrder,
    UnknownObservation,
    Serialization,
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "animation planning failed: {self:?}")
    }
}

impl std::error::Error for PlanError {}

#[cfg(test)]
mod tests {
    use chem_catalogue::CatalogueBundle;
    use chem_engine::{expand_structural_rule, structural_frames, validate_structural_reaction};

    use super::{
        EducationalCue, EducationalPlan, EducationalScene, EducationalSceneKind, RealWorldPosition,
        TimelinePosition, compile_educational_plan, compile_real_world_plan,
    };

    const SOURCE: &str = include_str!("../../../fixtures/silver-chloride.chems");
    const CATALOGUE: &[u8] =
        include_bytes!("../../../fixtures/catalogue/silver-chloride.catalogue.json");
    const LITHIUM_SOURCE: &str = include_str!("../../../fixtures/lithium-water.chems");
    const LITHIUM_CATALOGUE: &[u8] =
        include_bytes!("../../../fixtures/catalogue/lithium-water.catalogue.json");

    fn trusted_from(
        source: &str,
        catalogue_bytes: &[u8],
    ) -> (
        chem_engine::ValidatedStructuralReaction,
        Vec<chem_engine::StructuralFrame>,
    ) {
        let catalogue = CatalogueBundle::load_json(catalogue_bytes).expect("catalogue loads");
        let expanded = expand_structural_rule(source, &catalogue).expect("rule expands");
        let validated = validate_structural_reaction(expanded).expect("rule validates");
        let frames = structural_frames(&validated).expect("frames generate");
        (validated, frames)
    }

    fn trusted() -> (
        chem_engine::ValidatedStructuralReaction,
        Vec<chem_engine::StructuralFrame>,
    ) {
        trusted_from(SOURCE, CATALOGUE)
    }

    fn timeline_plan(durations_ms: &[u32]) -> EducationalPlan {
        let scenes = durations_ms
            .iter()
            .enumerate()
            .map(|(index, duration_ms)| {
                let id = chem_domain::ContentDigest::sha256(&index.to_le_bytes());
                EducationalScene {
                    kind: EducationalSceneKind::Introduction,
                    start_frame: id,
                    end_frame: id,
                    duration_ms: *duration_ms,
                    cues: Vec::new(),
                }
            })
            .collect();
        EducationalPlan {
            id: chem_domain::ContentDigest::sha256(b"timeline-plan"),
            reaction: chem_domain::ContentDigest::sha256(b"timeline-reaction"),
            scenes,
            event_model: "representative".to_owned(),
            sequence_model: "explanatory".to_owned(),
        }
    }

    #[test]
    fn educational_timeline_locates_variable_scenes_and_boundaries() {
        let plan = timeline_plan(&[1_000, 2_500, 500]);

        assert_eq!(plan.duration_ms(), 4_000);
        assert_eq!(
            plan.locate(0),
            Some(TimelinePosition {
                scene_index: 0,
                scene_elapsed_ms: 0,
            })
        );
        assert_eq!(
            plan.locate(999),
            Some(TimelinePosition {
                scene_index: 0,
                scene_elapsed_ms: 999,
            })
        );
        assert_eq!(
            plan.locate(1_000),
            Some(TimelinePosition {
                scene_index: 1,
                scene_elapsed_ms: 0,
            })
        );
        assert_eq!(
            plan.locate(3_499),
            Some(TimelinePosition {
                scene_index: 1,
                scene_elapsed_ms: 2_499,
            })
        );
        assert_eq!(
            plan.locate(3_500),
            Some(TimelinePosition {
                scene_index: 2,
                scene_elapsed_ms: 0,
            })
        );
        let end = TimelinePosition {
            scene_index: 2,
            scene_elapsed_ms: 500,
        };
        assert_eq!(plan.locate(4_000), Some(end));
        assert_eq!(plan.locate(u64::MAX), Some(end));
    }

    #[test]
    fn educational_timeline_handles_zero_duration_scenes_deterministically() {
        let plan = timeline_plan(&[0, 1_000, 0, 500, 0]);

        assert_eq!(plan.duration_ms(), 1_500);
        assert_eq!(
            plan.locate(0),
            Some(TimelinePosition {
                scene_index: 1,
                scene_elapsed_ms: 0,
            })
        );
        assert_eq!(
            plan.locate(1_000),
            Some(TimelinePosition {
                scene_index: 3,
                scene_elapsed_ms: 0,
            })
        );
        assert_eq!(
            plan.locate(1_500),
            Some(TimelinePosition {
                scene_index: 4,
                scene_elapsed_ms: 0,
            })
        );

        let all_zero = timeline_plan(&[0, 0]);
        assert_eq!(all_zero.duration_ms(), 0);
        assert_eq!(
            all_zero.locate(u64::MAX),
            Some(TimelinePosition {
                scene_index: 1,
                scene_elapsed_ms: 0,
            })
        );
        assert_eq!(timeline_plan(&[]).locate(0), None);
    }

    #[test]
    fn educational_timeline_elapsed_conversion_clamps_and_round_trips() {
        let plan = timeline_plan(&[1_000, 2_500, 500]);

        assert_eq!(
            plan.elapsed_at(TimelinePosition {
                scene_index: 1,
                scene_elapsed_ms: 1_250,
            }),
            Some(2_250)
        );
        assert_eq!(
            plan.elapsed_at(TimelinePosition {
                scene_index: 1,
                scene_elapsed_ms: u32::MAX,
            }),
            Some(3_500)
        );
        assert_eq!(
            plan.elapsed_at(TimelinePosition {
                scene_index: 3,
                scene_elapsed_ms: 0,
            }),
            None
        );

        for elapsed_ms in [0, 1, 999, 1_000, 2_275, 3_499, 3_500, 3_999, 4_000] {
            let position = plan.locate(elapsed_ms).expect("timeline position exists");
            assert_eq!(plan.elapsed_at(position), Some(elapsed_ms));
        }
    }

    #[test]
    fn educational_planning_is_deterministic_and_maps_every_operation() {
        let (validated, frames) = trusted();
        let first = compile_educational_plan(&validated, &frames).expect("plan compiles");
        let second = compile_educational_plan(&validated, &frames).expect("plan recompiles");
        assert_eq!(first, second);
        let operations = first
            .scenes
            .iter()
            .flat_map(|scene| &scene.cues)
            .filter(|cue| matches!(cue, EducationalCue::ApplyOperation { .. }))
            .count();
        assert_eq!(operations, frames.len() - 1);
        let structural_changes = first
            .scenes
            .iter()
            .filter(|scene| scene.kind == EducationalSceneKind::StructuralChange)
            .collect::<Vec<_>>();
        assert_eq!(structural_changes.len(), operations);
        assert!(structural_changes.iter().all(|scene| {
            scene.duration_ms >= 6_800
                && scene.cues.iter().any(|cue| {
                    matches!(
                        cue,
                        EducationalCue::ShowExplanation { label }
                            if label.connector && !label.text.is_empty()
                    )
                })
                && scene
                    .cues
                    .iter()
                    .any(|cue| matches!(cue, EducationalCue::ShowContext { .. }))
        }));
        assert!(
            first
                .scenes
                .iter()
                .all(|scene| scene.kind != EducationalSceneKind::ExplanationPause)
        );
        assert_eq!(first.scenes[0].kind, EducationalSceneKind::Introduction);
        let equation = first
            .scenes
            .iter()
            .flat_map(|scene| &scene.cues)
            .find_map(|cue| match cue {
                EducationalCue::ShowEquation { equation } => Some(equation),
                _ => None,
            })
            .expect("reviewed equation is planned");
        assert_eq!(equation.reactants[0].formula, "AgNO3");
        assert_eq!(equation.reactants[0].coefficient, 1);
        assert_eq!(
            first.scenes.last().map(|scene| scene.kind),
            Some(EducationalSceneKind::Summary)
        );
    }

    #[test]
    fn narration_is_deterministic_source_selected_and_free_of_internal_ids() {
        let (silver, silver_frames) = trusted();
        let (lithium, lithium_frames) = trusted_from(LITHIUM_SOURCE, LITHIUM_CATALOGUE);
        let silver_plan =
            compile_educational_plan(&silver, &silver_frames).expect("silver plan compiles");
        let lithium_plan =
            compile_educational_plan(&lithium, &lithium_frames).expect("lithium plan compiles");

        let visible_copy = |plan: &EducationalPlan| {
            plan.scenes
                .iter()
                .flat_map(|scene| &scene.cues)
                .filter_map(|cue| match cue {
                    EducationalCue::ShowContext { label } => {
                        Some(format!("{} {}", label.title, label.text))
                    }
                    EducationalCue::ShowExplanation { label } => Some(label.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let silver_copy = visible_copy(&silver_plan);
        let lithium_copy = visible_copy(&lithium_plan);

        assert!(silver_copy.contains("Ag"));
        assert!(silver_copy.contains("Cl"));
        assert!(silver_copy.contains("white"));
        assert!(lithium_copy.contains("Li → O"));
        assert!(lithium_copy.contains("H2 gas evolves"));
        assert!(lithium_copy.contains("LiOH"));
        assert_ne!(silver_copy, lithium_copy);
        for copy in [&silver_copy, &lithium_copy] {
            assert!(!copy.contains("species."));
            assert!(!copy.contains("atom."));
            assert!(!copy.contains("metal."));
        }
    }

    #[test]
    fn every_structural_change_carries_trusted_targeted_context() {
        let (validated, frames) = trusted_from(LITHIUM_SOURCE, LITHIUM_CATALOGUE);
        let plan = compile_educational_plan(&validated, &frames).expect("plan compiles");

        for scene in plan
            .scenes
            .iter()
            .filter(|scene| scene.kind == EducationalSceneKind::StructuralChange)
        {
            let affected = scene
                .cues
                .iter()
                .find_map(|cue| match cue {
                    EducationalCue::ApplyOperation { affected_atoms, .. } => Some(affected_atoms),
                    _ => None,
                })
                .expect("operation cue exists");
            let context = scene
                .cues
                .iter()
                .find_map(|cue| match cue {
                    EducationalCue::ShowContext { label } => Some(label),
                    _ => None,
                })
                .expect("context cue exists");
            assert_eq!(&context.target_atoms, affected);
            assert!(!context.text.is_empty());
            assert!(context.connector);
        }
    }

    #[test]
    fn real_world_plan_uses_reviewed_reusable_profiles() {
        let (validated, _) = trusted();
        let plan = compile_real_world_plan(&validated).expect("scene plan compiles");
        assert_eq!(plan.profile_id, "presentation.aqueous-precipitation");
        assert!(
            plan.objects
                .iter()
                .all(|object| !object.id.contains("Lithium"))
        );
        assert!(
            plan.effects
                .iter()
                .all(|effect| effect.trigger_observation.starts_with("observation."))
        );
    }

    #[test]
    fn real_world_planning_is_deterministic() {
        let (validated, _) = trusted_from(LITHIUM_SOURCE, LITHIUM_CATALOGUE);
        let first = compile_real_world_plan(&validated).expect("scene plan compiles");
        let second = compile_real_world_plan(&validated).expect("scene plan recompiles");

        assert_eq!(first, second);
        assert_eq!(first.id, second.id);
        assert_eq!(
            serde_json::to_vec(&first).expect("first plan serializes"),
            serde_json::to_vec(&second).expect("second plan serializes")
        );
    }

    #[test]
    fn real_world_timeline_locates_start_boundaries_and_exact_end() {
        let (validated, _) = trusted_from(LITHIUM_SOURCE, LITHIUM_CATALOGUE);
        let plan = compile_real_world_plan(&validated).expect("scene plan compiles");
        let timeline = &plan.timeline;

        assert!(!timeline.beats.is_empty());
        assert!(timeline.duration_ms() > 0);
        assert!(
            timeline
                .beats
                .iter()
                .all(|beat| { beat.duration_ms > 0 && beat.start_ordinal <= beat.end_ordinal })
        );

        let mut elapsed_ms = 0_u64;
        for (beat_index, beat) in timeline.beats.iter().enumerate() {
            assert_eq!(
                timeline.locate(elapsed_ms),
                Some(RealWorldPosition {
                    beat_index,
                    ordinal: beat.start_ordinal,
                    ordinal_progress: 0.0,
                    beat_progress: 0.0,
                }),
                "an exact beat boundary belongs to the following beat"
            );
            elapsed_ms = elapsed_ms.saturating_add(u64::from(beat.duration_ms));
        }

        assert_eq!(elapsed_ms, timeline.duration_ms());
        let final_index = timeline.beats.len() - 1;
        let final_beat = &timeline.beats[final_index];
        let end = RealWorldPosition {
            beat_index: final_index,
            ordinal: final_beat.end_ordinal,
            ordinal_progress: 1.0,
            beat_progress: 1.0,
        };
        assert_eq!(timeline.locate(timeline.duration_ms()), Some(end));
        assert_eq!(timeline.locate(u64::MAX), Some(end));
    }

    #[test]
    fn macroscopic_annotations_use_reviewed_formulae_without_internal_ids() {
        let (silver, _) = trusted();
        let (lithium, _) = trusted_from(LITHIUM_SOURCE, LITHIUM_CATALOGUE);
        let silver_plan = compile_real_world_plan(&silver).expect("silver scene plan compiles");
        let lithium_plan = compile_real_world_plan(&lithium).expect("lithium scene plan compiles");

        let annotation_copy = |plan: &super::ScenePlan| {
            plan.annotations
                .iter()
                .map(|annotation| format!("{} {}", annotation.title, annotation.text))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let silver_copy = annotation_copy(&silver_plan);
        let lithium_copy = annotation_copy(&lithium_plan);

        assert!(silver_copy.contains("AgNO3"));
        assert!(silver_copy.contains("AgCl"));
        assert!(silver_copy.contains("white"));
        assert!(lithium_copy.contains("H2O"));
        assert!(lithium_copy.contains("H2"));
        assert!(lithium_copy.contains("LiOH"));
        assert_ne!(silver_copy, lithium_copy);

        for copy in [&silver_copy, &lithium_copy] {
            assert!(!copy.contains("species."));
            assert!(!copy.contains("observation."));
            assert!(!copy.contains("atom."));
            assert!(!copy.contains("metal."));
        }
    }
}
