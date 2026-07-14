#![forbid(unsafe_code)]

//! Deterministic, renderer-independent animation planning.
//!
//! This crate owns explanatory pacing and macroscopic scene composition. It
//! accepts only a trusted [`ValidatedStructuralReaction`]; it never parses
//! `.chems`, resolves catalogue data, or infers chemistry.

use std::collections::BTreeSet;
use std::fmt;

use chem_catalogue::{
    AssetProfile, CameraCue, ObservationClaim, PresentationEffect, PresentationObject,
    ReviewedEquation, StructuralOperation,
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
        scenes.push(EducationalScene {
            kind: EducationalSceneKind::StructuralChange,
            start_frame: before.id,
            end_frame: after.id,
            duration_ms: operation_duration(&operation),
            cues: vec![
                EducationalCue::EstablishFrame { frame: before.id },
                EducationalCue::ApplyOperation {
                    affected_atoms: affected_atoms(&operation),
                    operation: operation.clone(),
                    before: before.id,
                    after: after.id,
                },
            ],
        });
        scenes.push(explanation_scene(after, &operation));
        let observations = after
            .observations
            .iter()
            .filter(|observation| observation.stage == ObservationStage::Active)
            .map(|observation| {
                let label = ExplanationLabel {
                    kind: ExplanationLabelKind::ObservationExplanation,
                    text: observation_explanation(&observation.observation.claim),
                    target_atoms: product_atoms(
                        after,
                        observation_species(&observation.observation.claim),
                    ),
                    connector: true,
                };
                EducationalScene {
                    kind: EducationalSceneKind::ObservationConnection,
                    start_frame: after.id,
                    end_frame: after.id,
                    duration_ms: explanation_duration(&label.text),
                    cues: vec![
                        EducationalCue::ShowObservation {
                            observation_id: observation.observation.id.clone(),
                            frame: after.id,
                        },
                        EducationalCue::ShowExplanation { label },
                    ],
                }
            })
            .collect::<Vec<_>>();
        scenes.extend(observations);
    }

    scenes.push(summary_scene(reaction, last));

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

fn explanation_scene(frame: &StructuralFrame, operation: &StructuralOperation) -> EducationalScene {
    let text = operation_explanation(operation);
    EducationalScene {
        kind: EducationalSceneKind::ExplanationPause,
        start_frame: frame.id,
        end_frame: frame.id,
        duration_ms: explanation_duration(&text),
        cues: vec![EducationalCue::ShowExplanation {
            label: ExplanationLabel {
                kind: ExplanationLabelKind::StructuralChangeExplanation,
                text,
                target_atoms: affected_atoms(operation),
                connector: true,
            },
        }],
    }
}

fn operation_explanation(operation: &StructuralOperation) -> String {
    match operation {
        StructuralOperation::AssociateIonic { .. } => {
            "Oppositely charged partners are now associated.".to_owned()
        }
        StructuralOperation::AssignProduct { .. } => {
            "These conserved atoms are now assigned to a validated product.".to_owned()
        }
        StructuralOperation::TransferMetallicElectron { .. } => {
            "A delocalised electron transfers to the accepting atom.".to_owned()
        }
        StructuralOperation::CleaveCovalent { .. } => {
            "This shared covalent relationship has been cleaved.".to_owned()
        }
        StructuralOperation::FormCovalent { .. } => {
            "A new shared electron pair forms this covalent bond.".to_owned()
        }
    }
}

fn observation_explanation(claim: &ObservationClaim) -> String {
    match claim {
        ObservationClaim::ProductForms { .. } => {
            "A validated product is now established.".to_owned()
        }
        ObservationClaim::ProductHasColour { colour, .. } => {
            format!("The product is observed as {colour}.")
        }
        ObservationClaim::GasEvolves { .. } => "Gas evolution is now observable.".to_owned(),
        ObservationClaim::ReactantConsumed { .. } => {
            "The reactant is visibly consumed as the event develops.".to_owned()
        }
    }
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
    frame
        .product_memberships
        .iter()
        .find(|membership| membership.product == species)
        .map_or_else(Vec::new, |membership| membership.atoms.clone())
}

fn explanation_duration(text: &str) -> u32 {
    let words = u32::try_from(text.split_whitespace().count()).unwrap_or(u32::MAX);
    (1_900_u32.saturating_add(words.saturating_mul(170))).clamp(2_800, 5_200)
}

fn introductory_scenes(
    reaction: &ValidatedStructuralReaction,
    first: &StructuralFrame,
) -> Vec<EducationalScene> {
    vec![
        EducationalScene {
            kind: EducationalSceneKind::Introduction,
            start_frame: first.id,
            end_frame: first.id,
            duration_ms: 2_400,
            cues: vec![EducationalCue::PreserveDisclosure {
                event_model: first.event_model.clone(),
                sequence_model: first.sequence_model.clone(),
            }],
        },
        EducationalScene {
            kind: EducationalSceneKind::ReactantSetup,
            start_frame: first.id,
            end_frame: first.id,
            duration_ms: 3_200,
            cues: vec![
                EducationalCue::IntroduceSpecies {
                    role: SpeciesRole::Reactant,
                    species: reaction.reactants().to_vec(),
                },
                EducationalCue::EstablishFrame { frame: first.id },
            ],
        },
        EducationalScene {
            kind: EducationalSceneKind::Equation,
            start_frame: first.id,
            end_frame: first.id,
            duration_ms: 3_000,
            cues: vec![EducationalCue::ShowEquation {
                equation: reaction.equation().clone(),
            }],
        },
    ]
}

fn summary_scene(
    reaction: &ValidatedStructuralReaction,
    last: &StructuralFrame,
) -> EducationalScene {
    EducationalScene {
        kind: EducationalSceneKind::Summary,
        start_frame: last.id,
        end_frame: last.id,
        duration_ms: 3_600,
        cues: vec![
            EducationalCue::IntroduceSpecies {
                role: SpeciesRole::Product,
                species: reaction.products().to_vec(),
            },
            EducationalCue::ShowEquation {
                equation: reaction.equation().clone(),
            },
            EducationalCue::EstablishFrame { frame: last.id },
            EducationalCue::ShowExplanation {
                label: ExplanationLabel {
                    kind: ExplanationLabelKind::SummaryExplanation,
                    text: "The validated products and observations are now established.".to_owned(),
                    target_atoms: last.atoms.iter().map(|atom| atom.id.clone()).collect(),
                    connector: false,
                },
            },
        ],
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
    let identity =
        serde_json::to_value((reaction.digest(), profile)).map_err(|_| PlanError::Serialization)?;
    Ok(ScenePlan {
        id: ContentDigest::of_json(&identity).map_err(|_| PlanError::Serialization)?,
        reaction: reaction.digest(),
        profile_id: profile.id.clone(),
        environment: profile.environment,
        objects: profile.objects.clone(),
        effects: profile.effects.clone(),
        camera: profile.camera.clone(),
        disclosure: profile.disclosure.clone(),
        virtual_only_disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    })
}

fn operation_duration(operation: &StructuralOperation) -> u32 {
    match operation {
        StructuralOperation::AssociateIonic { .. } => 3_400,
        StructuralOperation::AssignProduct { .. } => 2_600,
        StructuralOperation::TransferMetallicElectron { .. } => 4_200,
        StructuralOperation::CleaveCovalent { .. } | StructuralOperation::FormCovalent { .. } => {
            3_800
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
        EducationalCue, EducationalSceneKind, compile_educational_plan, compile_real_world_plan,
    };

    const SOURCE: &str = include_str!("../../../fixtures/silver-chloride.chems");
    const CATALOGUE: &[u8] =
        include_bytes!("../../../fixtures/catalogue/silver-chloride.catalogue.json");

    fn trusted() -> (
        chem_engine::ValidatedStructuralReaction,
        Vec<chem_engine::StructuralFrame>,
    ) {
        let catalogue = CatalogueBundle::load_json(CATALOGUE).expect("catalogue loads");
        let expanded = expand_structural_rule(SOURCE, &catalogue).expect("rule expands");
        let validated = validate_structural_reaction(expanded).expect("rule validates");
        let frames = structural_frames(&validated).expect("frames generate");
        (validated, frames)
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
        let explanation_pauses = first
            .scenes
            .iter()
            .filter(|scene| scene.kind == EducationalSceneKind::ExplanationPause)
            .collect::<Vec<_>>();
        assert_eq!(explanation_pauses.len(), operations);
        assert!(explanation_pauses.iter().all(|scene| {
            scene.duration_ms >= 2_800
                && scene.cues.iter().any(|cue| {
                    matches!(
                        cue,
                        EducationalCue::ShowExplanation { label }
                            if label.connector && !label.text.is_empty()
                    )
                })
        }));
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
}
