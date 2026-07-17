#![forbid(unsafe_code)]

//! Deterministic, renderer-independent planning over trusted kernel frames.
//!
//! This crate owns pacing and macroscopic scene composition. It never parses
//! `.chems`, resolves rules, or constructs chemistry.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use chem_catalogue::ObservationPredicate;
use chem_domain::{
    AtomId, ContentDigest, IonicAssociationId, Phase, RepresentationKind, StructuralOperationView,
};
use chem_kernel::{FrameObservation, ObservationStatus, SimulationFrame, SimulationFrames};

pub const VIRTUAL_ONLY_DISCLOSURE: &str = "Virtual educational model—not a laboratory procedure. Timing, scale, and motion are illustrative; the fixed 2.5D camera is a presentation view.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EducationalSceneKind {
    Introduction,
    ReactantSetup,
    Equation,
    StructuralChange,
    ExplanationPause,
    ObservationConnection,
    Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplanationLabelKind {
    ConceptExplanation,
    StructuralChangeExplanation,
    ObservationExplanation,
    EquationExplanation,
    ImportantResult,
    SummaryExplanation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplanationLabel {
    pub kind: ExplanationLabelKind,
    pub text: String,
    pub target_atoms: Vec<String>,
    pub connector: bool,
}

/// Concise deterministic copy displayed beside trusted structural content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLabel {
    pub kind: ExplanationLabelKind,
    pub title: String,
    pub text: String,
    pub target_atoms: Vec<String>,
    pub connector: bool,
}

/// One exact trusted transition included in an educational action beat.
///
/// A beat may contain several independent, equivalent transitions. Keeping
/// every boundary digest lets renderers animate them together without
/// merging or rewriting the validated frame sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EducationalOperation {
    pub before: ContentDigest,
    pub after: ContentDigest,
    pub affected_atoms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EducationalCue {
    EstablishFrame {
        frame: ContentDigest,
    },
    ApplyOperations {
        operations: Vec<EducationalOperation>,
    },
    ShowEquation {
        equation: String,
    },
    ShowObservation {
        predicate: ObservationPredicate,
        frame: ContentDigest,
    },
    ShowExplanation {
        label: ExplanationLabel,
    },
    ShowContext {
        label: ContextLabel,
    },
    PreserveDisclosure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EducationalScene {
    pub kind: EducationalSceneKind,
    pub start_frame: ContentDigest,
    pub end_frame: ContentDigest,
    pub duration_ms: u32,
    pub cues: Vec<EducationalCue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EducationalPlan {
    pub id: ContentDigest,
    pub scenes: Vec<EducationalScene>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelinePosition {
    pub scene_index: usize,
    pub scene_elapsed_ms: u32,
}

impl EducationalPlan {
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.scenes.iter().fold(0_u64, |duration, scene| {
            duration.saturating_add(u64::from(scene.duration_ms))
        })
    }

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

/// Adds explanatory pacing around the immutable operation sequence without
/// changing or reordering any chemical state.
///
/// # Errors
///
/// Returns an error if frames are absent, non-contiguous, missing their
/// validated operation, or cannot provide their bound generation digest.
#[allow(clippy::too_many_lines)]
pub fn compile_educational_plan(frames: &SimulationFrames) -> Result<EducationalPlan, PlanError> {
    let sequence = frames.frames();
    let first = sequence.first().ok_or(PlanError::MissingFrames)?;
    let last = sequence.last().ok_or(PlanError::MissingFrames)?;
    if sequence
        .iter()
        .enumerate()
        .any(|(index, frame)| frame.ordinal() != u32::try_from(index).unwrap_or(u32::MAX))
    {
        return Err(PlanError::InvalidFrameSequence);
    }

    let mut scenes = vec![
        scene(
            EducationalSceneKind::Introduction,
            first,
            first,
            3_000,
            vec![EducationalCue::PreserveDisclosure],
        ),
        scene(
            EducationalSceneKind::ReactantSetup,
            first,
            first,
            4_000,
            vec![EducationalCue::EstablishFrame {
                frame: first.trace().state_digest,
            }],
        ),
        scene(
            EducationalSceneKind::Equation,
            first,
            first,
            3_800,
            Vec::new(),
        ),
    ];

    let mut transition_index = 1;
    while transition_index < sequence.len() {
        let group_start = transition_index;
        let before = &sequence[group_start - 1];
        let first_after = &sequence[group_start];
        let first_operation = first_after
            .active_operation()
            .ok_or(PlanError::MissingOperation(first_after.ordinal()))?;
        let signature = operation_signature(before, first_after, first_operation.operation.view());
        let first_narration =
            operation_narration(before, first_after, first_operation.operation.view());
        let mut affected = first_narration
            .explanation
            .target_atoms
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut group_end = group_start;

        while group_end + 1 < sequence.len() && !has_active_observation(&sequence[group_end]) {
            let candidate_before = &sequence[group_end];
            let candidate_after = &sequence[group_end + 1];
            let candidate_operation = candidate_after
                .active_operation()
                .ok_or(PlanError::MissingOperation(candidate_after.ordinal()))?;
            let candidate_narration = operation_narration(
                candidate_before,
                candidate_after,
                candidate_operation.operation.view(),
            );
            let candidate_atoms = candidate_narration
                .explanation
                .target_atoms
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            if operation_signature(
                candidate_before,
                candidate_after,
                candidate_operation.operation.view(),
            ) != signature
                || !affected.is_disjoint(&candidate_atoms)
            {
                break;
            }
            affected.extend(candidate_atoms);
            group_end += 1;
        }

        let after = &sequence[group_end];
        // One representative instance owns the callout target. The complete
        // atom union remains available on the exact operation cues for
        // simultaneous animation, but is not averaged into a marker between
        // repeated reactant or product instances.
        let narration = first_narration;
        let before_digest = before.trace().state_digest;
        let duration_ms =
            3_200_u32.saturating_add(explanation_duration(&narration.explanation.text));
        let operations = (group_start..=group_end)
            .map(|index| {
                let operation_before = &sequence[index - 1];
                let operation_after = &sequence[index];
                let active = operation_after
                    .active_operation()
                    .ok_or(PlanError::MissingOperation(operation_after.ordinal()))?;
                let narration =
                    operation_narration(operation_before, operation_after, active.operation.view());
                Ok(EducationalOperation {
                    before: operation_before.trace().state_digest,
                    after: operation_after.trace().state_digest,
                    affected_atoms: narration.explanation.target_atoms,
                })
            })
            .collect::<Result<Vec<_>, PlanError>>()?;
        scenes.push(scene(
            EducationalSceneKind::StructuralChange,
            before,
            after,
            duration_ms,
            vec![
                EducationalCue::EstablishFrame {
                    frame: before_digest,
                },
                EducationalCue::ApplyOperations { operations },
                EducationalCue::ShowContext {
                    label: narration.context,
                },
                EducationalCue::ShowExplanation {
                    label: narration.explanation,
                },
            ],
        ));

        transition_index = group_end + 1;
    }

    scenes.push(scene(
        EducationalSceneKind::Summary,
        last,
        last,
        4_800,
        vec![EducationalCue::EstablishFrame {
            frame: last.trace().state_digest,
        }],
    ));
    Ok(EducationalPlan {
        id: frames.digest().map_err(|_| PlanError::Digest)?,
        scenes,
    })
}

fn has_active_observation(frame: &SimulationFrame) -> bool {
    frame
        .observations()
        .iter()
        .any(|observation| observation.status == ObservationStatus::Active)
}

#[allow(clippy::too_many_lines)]
fn operation_signature(
    before: &SimulationFrame,
    after: &SimulationFrame,
    operation: StructuralOperationView<'_>,
) -> String {
    match operation {
        StructuralOperationView::ReconfigureElectrons { transition } => format!(
            "reconfigure:{}:{}",
            atom_symbol(before, after, transition.atom()),
            atom_delta_signature(before, after, [transition.atom()])
        ),
        StructuralOperationView::CleaveCovalent {
            left,
            right,
            expected_order,
            ..
        } => format!(
            "cleave:{}:{}:{}:{}",
            atom_symbol(before, after, left),
            atom_symbol(before, after, right),
            expected_order.order(),
            atom_delta_signature(before, after, [left, right])
        ),
        StructuralOperationView::FormCovalent {
            left, right, order, ..
        } => format!(
            "form:{}:{}:{}:{}",
            atom_symbol(before, after, left),
            atom_symbol(before, after, right),
            order.order(),
            atom_delta_signature(before, after, [left, right])
        ),
        StructuralOperationView::CleaveDative {
            donor, acceptor, ..
        } => format!(
            "cleave-dative:{}:{}:{}",
            atom_symbol(before, after, donor),
            atom_symbol(before, after, acceptor),
            atom_delta_signature(before, after, [donor, acceptor])
        ),
        StructuralOperationView::FormDative {
            donor, acceptor, ..
        } => format!(
            "form-dative:{}:{}:{}",
            atom_symbol(before, after, donor),
            atom_symbol(before, after, acceptor),
            atom_delta_signature(before, after, [donor, acceptor])
        ),
        StructuralOperationView::ChangeCovalent {
            left,
            right,
            old_order,
            new_order,
            ..
        } => format!(
            "change:{}:{}:{}:{}:{}",
            atom_symbol(before, after, left),
            atom_symbol(before, after, right),
            old_order.order(),
            new_order.order(),
            atom_delta_signature(before, after, [left, right])
        ),
        StructuralOperationView::ChangeCovalentDelocalization {
            left,
            right,
            expected,
            replacement,
        } => format!(
            "delocalize:{}:{}:{}:{}",
            atom_symbol(before, after, left),
            atom_symbol(before, after, right),
            delocalization_name(expected),
            delocalization_name(replacement),
        ),
        StructuralOperationView::AssociateIonic { association } => {
            let mut components = association
                .components()
                .iter()
                .filter_map(|group| after.groups().get(group))
                .map(|group| {
                    let mut symbols = group
                        .atoms
                        .iter()
                        .map(|atom| atom_symbol(before, after, atom))
                        .collect::<Vec<_>>();
                    symbols.sort();
                    let charge = after
                        .ionic_associations()
                        .get(association.id())
                        .and_then(|association| association.component_charges.get(&group.id))
                        .copied()
                        .unwrap_or(0);
                    format!("{charge}:{}", symbols.join("."))
                })
                .collect::<Vec<_>>();
            components.sort();
            format!("associate:{}", components.join("+"))
        }
        StructuralOperationView::DissociateIonic { association } => {
            let mut components = before
                .ionic_associations()
                .get(association)
                .into_iter()
                .flat_map(|association| association.components.values())
                .map(|atoms| {
                    let mut symbols = atoms
                        .iter()
                        .map(|atom| atom_symbol(before, after, atom))
                        .collect::<Vec<_>>();
                    symbols.sort();
                    symbols.join(".")
                })
                .collect::<Vec<_>>();
            components.sort();
            format!("dissociate:{}", components.join("+"))
        }
        StructuralOperationView::ReleaseMetallic { site, .. } => {
            format!(
                "release:{}:{}",
                atom_symbol(before, after, site),
                atom_delta_signature(before, after, [site])
            )
        }
        StructuralOperationView::JoinMetallic { site, .. } => {
            format!(
                "join:{}:{}",
                atom_symbol(before, after, site),
                atom_delta_signature(before, after, [site])
            )
        }
        StructuralOperationView::TransferElectron {
            donor,
            acceptor,
            count,
            ..
        } => format!(
            "transfer:{}:{}:{count}:{}",
            atom_symbol(before, after, donor),
            atom_symbol(before, after, acceptor),
            atom_delta_signature(before, after, [donor, acceptor])
        ),
        StructuralOperationView::AssignProduct { atoms, .. } => {
            let mut symbols = atoms
                .iter()
                .map(|atom| atom_symbol(before, after, atom))
                .collect::<Vec<_>>();
            symbols.sort();
            format!("assign:{}", symbols.join("."))
        }
    }
}

fn atom_delta_signature<'a>(
    before_frame: &SimulationFrame,
    after_frame: &SimulationFrame,
    atoms: impl IntoIterator<Item = &'a AtomId>,
) -> String {
    atoms
        .into_iter()
        .map(|atom| {
            let before = before_frame.atoms().get(atom).map(|atom| atom.electrons);
            let after = after_frame.atoms().get(atom).map(|atom| atom.electrons);
            format!(
                "{}:{}>{}",
                atom_symbol(before_frame, after_frame, atom),
                before.map_or_else(|| "missing".to_owned(), electron_state_signature),
                after.map_or_else(|| "missing".to_owned(), electron_state_signature)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn electron_state_signature(state: chem_domain::ElectronState) -> String {
    format!(
        "{}/{}/{}",
        state.formal_charge(),
        state.non_bonding_electrons(),
        state.unpaired_electrons()
    )
}

fn scene(
    kind: EducationalSceneKind,
    start: &SimulationFrame,
    end: &SimulationFrame,
    duration_ms: u32,
    cues: Vec<EducationalCue>,
) -> EducationalScene {
    EducationalScene {
        kind,
        start_frame: start.trace().state_digest,
        end_frame: end.trace().state_digest,
        duration_ms,
        cues,
    }
}

#[derive(Debug)]
struct OperationNarration {
    context: ContextLabel,
    explanation: ExplanationLabel,
}

// Exhaustive operation coverage belongs together so new structural operations
// cannot acquire narration without an explicit context/explanation decision.
#[allow(clippy::too_many_lines)]
fn operation_narration(
    before: &SimulationFrame,
    after: &SimulationFrame,
    operation: StructuralOperationView<'_>,
) -> OperationNarration {
    let (context, explanation, target_atoms) = match operation {
        StructuralOperationView::ReconfigureElectrons { transition } => (
            format!(
                "{} electron occupancy reorganises",
                atom_symbol(before, after, transition.atom())
            ),
            "Local electrons change pairing so the next reviewed bond change can occur."
                .to_owned(),
            atom_targets([transition.atom()]),
        ),
        StructuralOperationView::CleaveCovalent {
            left,
            right,
            expected_order,
            ..
        } => (
            format!(
                "{}–{} {} bond separates",
                atom_symbol(before, after, left),
                atom_symbol(before, after, right),
                bond_order_name(expected_order.order())
            ),
            "The validated allocation moves the former bonding electrons out of the shared bond."
                .to_owned(),
            atom_targets([left, right]),
        ),
        StructuralOperationView::FormCovalent {
            left, right, order, ..
        } => (
            format!(
                "{}–{} {} bond forms",
                atom_symbol(before, after, left),
                atom_symbol(before, after, right),
                bond_order_name(order.order())
            ),
            "A new shared electron pair forms this covalent bond.".to_owned(),
            atom_targets([left, right]),
        ),
        StructuralOperationView::CleaveDative {
            donor, acceptor, ..
        } => (
            format!(
                "{} → {} coordinate bond separates",
                atom_symbol(before, after, donor),
                atom_symbol(before, after, acceptor)
            ),
            "The coordinate bond is cleaved while its electron origin remains explicit.".to_owned(),
            atom_targets([donor, acceptor]),
        ),
        StructuralOperationView::FormDative {
            donor, acceptor, ..
        } => (
            format!(
                "{} → {} coordinate bond forms",
                atom_symbol(before, after, donor),
                atom_symbol(before, after, acceptor)
            ),
            "The donor supplies both electrons to this coordinate bond.".to_owned(),
            atom_targets([donor, acceptor]),
        ),
        StructuralOperationView::ChangeCovalent {
            left,
            right,
            old_order,
            new_order,
            ..
        } => (
            format!(
                "{}–{} bond order: {} → {}",
                atom_symbol(before, after, left),
                atom_symbol(before, after, right),
                old_order.order(),
                new_order.order()
            ),
            "The new order changes how many electron pairs are shared between these atoms."
                .to_owned(),
            atom_targets([left, right]),
        ),
        StructuralOperationView::ChangeCovalentDelocalization {
            left,
            right,
            expected,
            replacement,
        } => (
            format!(
                "{}â€“{} effective bond order: {} â†’ {}",
                atom_symbol(before, after, left),
                atom_symbol(before, after, right),
                delocalization_name(expected),
                delocalization_name(replacement),
            ),
            "The localized Lewis edge is retained while resonance delocalisation changes its validated effective bond order."
                .to_owned(),
            atom_targets([left, right]),
        ),
        StructuralOperationView::AssociateIonic { association } => {
            let target_atoms = ionic_targets(after, association.id());
            (
                count_phrase(
                    association.components().len(),
                    "charged component",
                    "associate",
                ),
                "Electrostatic attraction groups the oppositely charged components without representing a covalent bond."
                    .to_owned(),
                target_atoms,
            )
        }
        StructuralOperationView::DissociateIonic { association } => {
            let target_atoms = ionic_targets(before, association);
            (
                "Ionic components separate".to_owned(),
                "Electrostatic attraction no longer keeps these charged components in one association."
                    .to_owned(),
                target_atoms,
            )
        }
        StructuralOperationView::ReleaseMetallic { site, .. } => (
            format!(
                "{} leaves the metallic domain",
                atom_symbol(before, after, site)
            ),
            "A site leaves the shared metallic electron domain.".to_owned(),
            atom_targets([site]),
        ),
        StructuralOperationView::JoinMetallic { site, .. } => (
            format!(
                "{} joins the metallic domain",
                atom_symbol(before, after, site)
            ),
            "A site joins the shared metallic electron domain.".to_owned(),
            atom_targets([site]),
        ),
        StructuralOperationView::TransferElectron {
            donor,
            acceptor,
            count,
            ..
        } => (
            format!(
                "{} → {} · {count} {}",
                atom_symbol(before, after, donor),
                atom_symbol(before, after, acceptor),
                plural(count.into(), "electron")
            ),
            "The donor and acceptor electron states change while the total electron count remains conserved."
                .to_owned(),
            atom_targets([donor, acceptor]),
        ),
        StructuralOperationView::AssignProduct { atoms, .. } => (
            count_phrase(atoms.len(), "conserved atom", "assigned"),
            "Product assignment records the final validated grouping without creating or deleting atoms."
                .to_owned(),
            atoms
                .iter()
                .map(AtomId::as_str)
                .map(str::to_owned)
                .collect(),
        ),
    };
    let connector = !target_atoms.is_empty();
    let kind = ExplanationLabelKind::StructuralChangeExplanation;
    OperationNarration {
        context: ContextLabel {
            kind,
            title: operation_title(operation).to_owned(),
            text: context,
            target_atoms: target_atoms.clone(),
            connector,
        },
        explanation: ExplanationLabel {
            kind,
            text: explanation,
            target_atoms,
            connector,
        },
    }
}

fn atom_symbol(before: &SimulationFrame, after: &SimulationFrame, atom: &AtomId) -> String {
    after
        .atoms()
        .get(atom)
        .or_else(|| before.atoms().get(atom))
        .map_or_else(
            || "Atom".to_owned(),
            |atom| atom.element.as_str().to_owned(),
        )
}

fn atom_targets<'a>(atoms: impl IntoIterator<Item = &'a AtomId>) -> Vec<String> {
    atoms
        .into_iter()
        .map(AtomId::as_str)
        .map(str::to_owned)
        .collect()
}

fn ionic_targets(frame: &SimulationFrame, association: &IonicAssociationId) -> Vec<String> {
    frame
        .ionic_associations()
        .get(association)
        .into_iter()
        .flat_map(|association| association.components.values())
        .flatten()
        .map(AtomId::as_str)
        .map(str::to_owned)
        .collect()
}

const fn bond_order_name(order: u8) -> &'static str {
    match order {
        1 => "single",
        2 => "double",
        3 => "triple",
        _ => "changed",
    }
}

fn delocalization_name(value: Option<&chem_domain::CovalentDelocalization>) -> String {
    value.map_or_else(
        || "localized".to_owned(),
        |value| {
            let order = value.effective_order();
            format!("{}/{} delocalised", order.numerator(), order.denominator())
        },
    )
}

fn count_phrase(count: usize, subject: &str, verb: &str) -> String {
    format!("{count} {} {verb}", plural(count, subject))
}

fn plural(count: usize, singular: &str) -> String {
    if count == 1 {
        singular.to_owned()
    } else {
        format!("{singular}s")
    }
}

const fn operation_title(operation: StructuralOperationView<'_>) -> &'static str {
    match operation {
        StructuralOperationView::ReconfigureElectrons { .. } => "ELECTRON REORGANISATION",
        StructuralOperationView::CleaveCovalent { .. } => "BOND CLEAVAGE",
        StructuralOperationView::FormCovalent { .. } => "COVALENT BOND",
        StructuralOperationView::CleaveDative { .. } => "COORDINATE BOND CLEAVAGE",
        StructuralOperationView::FormDative { .. } => "COORDINATE BOND",
        StructuralOperationView::ChangeCovalent { .. } => "BOND ORDER",
        StructuralOperationView::ChangeCovalentDelocalization { .. } => "DELOCALISATION",
        StructuralOperationView::AssociateIonic { .. } => "IONIC ASSOCIATION",
        StructuralOperationView::DissociateIonic { .. } => "IONIC DISSOCIATION",
        StructuralOperationView::ReleaseMetallic { .. } => "METALLIC ELECTRON RELEASE",
        StructuralOperationView::JoinMetallic { .. } => "METALLIC DOMAIN",
        StructuralOperationView::TransferElectron { .. } => "ELECTRON TRANSFER",
        StructuralOperationView::AssignProduct { .. } => "PRODUCT ESTABLISHED",
    }
}

fn observation_text(predicate: ObservationPredicate, value: Option<&str>) -> String {
    match predicate {
        ObservationPredicate::Evolves => {
            "A gaseous product has formed and can leave the reaction mixture.".to_owned()
        }
        ObservationPredicate::Disappears => {
            "Consumption connects structural reaction progress to a visible decrease in reactant."
                .to_owned()
        }
        ObservationPredicate::Forms => {
            "The final trusted grouping now matches a validated product structure.".to_owned()
        }
        ObservationPredicate::Colour => value.map_or_else(
            || "The active colour observation connects the trusted outcome to a macroscopic visual change."
                .to_owned(),
            |colour| {
                format!(
                    "The {colour} observation connects the trusted outcome to a macroscopic visual change."
                )
            },
        ),
    }
}

const fn observation_title(predicate: ObservationPredicate) -> &'static str {
    match predicate {
        ObservationPredicate::Evolves => "GAS EVOLUTION",
        ObservationPredicate::Disappears => "REACTANT CONSUMED",
        ObservationPredicate::Forms => "PRODUCT FORMED",
        ObservationPredicate::Colour => "COLOUR OBSERVATION",
    }
}

fn explanation_duration(text: &str) -> u32 {
    let words = u32::try_from(text.split_whitespace().count()).unwrap_or(u32::MAX);
    (2_500_u32.saturating_add(words.saturating_mul(200))).clamp(3_600, 6_400)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetProfile {
    LaboratoryBench,
    DarkPresentationPlatform,
    Beaker,
    TestTube,
    ConicalFlask,
    MeasuringCylinder,
    MetalChunk,
    MetalStrip,
    CrystalCluster,
    PowderPile,
    LiquidVolume,
    PrecipitateCloud,
    GasCloud,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneRole {
    Environment,
    Vessel,
    Reactant,
    Product,
    Contents,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceProfile {
    LaboratoryNeutral,
    ClearGlass,
    Water,
    AqueousColourless,
    WhitePrecipitate,
    CreamPrecipitate,
    YellowPrecipitate,
    AlkaliMetal,
    MetalSilver,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationTransform {
    pub translation: [i16; 3],
    pub rotation: [i16; 3],
    pub scale: [u16; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationObject {
    pub id: String,
    pub asset: AssetProfile,
    pub semantic_identity: String,
    pub appearance: AppearanceProfile,
    pub role: SceneRole,
    pub transform: PresentationTransform,
    pub visible_from_ordinal: u16,
    pub observation: Option<ObjectObservationBinding>,
    pub colour_transition: Option<PresentationColourTransition>,
}

/// A trusted observation that must activate before an object may be shown.
/// An expected value closes the binding over value-bearing predicates such as
/// precipitate colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectObservationBinding {
    pub predicate: ObservationPredicate,
    pub value: Option<String>,
}

/// An sRGB display colour selected only after an exact typed colour
/// observation survives catalogue and kernel validation. Opacity remains a
/// material/phase concern so the same colour works for solids, liquids, and
/// gases without turning every phase opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualColour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// Binds a reusable colour transition to one exact validated `.chems`
/// observation. The subject and value prevent a colour belonging to one
/// product from leaking onto another product in the same reaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationColourTransition {
    pub subject_binding: String,
    pub value: String,
    pub target: VisualColour,
    pub start_ordinal: u16,
}

/// Resolve the visual interpretation of a reviewed `.chems` colour value.
/// Common named colours are supported directly. `Rgb.HexRRGGBB` uses the
/// existing qualified-name grammar and provides an exact arbitrary sRGB value
/// without changing `.chems 1` syntax.
#[must_use]
pub fn visual_colour(value: &str) -> Option<VisualColour> {
    let named = match value {
        "Colourless" => [0xd8, 0xe3, 0xe8],
        "White" => [0xf0, 0xf5, 0xfa],
        "Cream" => [0xf0, 0xe0, 0xad],
        "Yellow" => [0xef, 0xd1, 0x47],
        "Amber" => [0xe4, 0x9b, 0x2f],
        "Orange" => [0xe9, 0x7b, 0x32],
        "Red" => [0xd8, 0x4a, 0x4a],
        "Crimson" => [0xb9, 0x2f, 0x52],
        "Pink" => [0xe5, 0x83, 0xae],
        "Purple" => [0x8c, 0x62, 0xc7],
        "Violet" => [0x75, 0x55, 0xc7],
        "Blue" => [0x4d, 0x83, 0xc6],
        "Cyan" => [0x49, 0xb9, 0xc2],
        "Green" => [0x56, 0xa7, 0x68],
        "Olive" => [0x88, 0x8a, 0x45],
        "Brown" => [0x8b, 0x5f, 0x43],
        "Grey" | "Gray" => [0x8b, 0x96, 0xa0],
        "Black" => [0x18, 0x1b, 0x1f],
        value => return parse_hex_visual_colour(value),
    };
    Some(VisualColour {
        red: named[0],
        green: named[1],
        blue: named[2],
    })
}

fn parse_hex_visual_colour(value: &str) -> Option<VisualColour> {
    let digits = value.strip_prefix("Rgb.Hex")?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(VisualColour {
        red: u8::from_str_radix(&digits[0..2], 16).ok()?,
        green: u8::from_str_radix(&digits[2..4], 16).ok()?,
        blue: u8::from_str_radix(&digits[4..6], 16).ok()?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectProfile {
    BubbleEmitter,
    GasRelease,
    SurfaceDisturbance,
    LiquidMixing,
    ObjectShrinkage,
    PrecipitateFormation,
    Clouding,
    ColourTransition,
    SplashEmitter,
    HeatDistortion,
    FlameEmitter(FlamePalette),
}

/// Reviewed flame-colour families available to the generic flame renderer.
///
/// Selecting a palette does not assert that a reaction ignites. A trusted
/// presentation profile must still authorize `FlameEmitter` and bind it to an
/// observation before the renderer can display it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlamePalette {
    Natural,
    Crimson,
    YellowOrange,
    Lilac,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EffectIntensity {
    Subtle,
    Moderate,
    Strong,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationEffect {
    pub effect: EffectProfile,
    pub trigger: ObservationPredicate,
    pub intensity: EffectIntensity,
    pub start_ordinal: u16,
    pub end_ordinal: u16,
}

/// Continuous, renderer-independent macroscopic controls compiled from the
/// currently active, observation-gated presentation effects.
///
/// Values are normalized illustrative intensities in `0.0..=1.0`, not measured
/// kinetic, thermodynamic, or pressure quantities. Missing reviewed metadata
/// deliberately remains zero instead of being inferred from a chemical name.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReactionVisualInputs {
    pub reaction_progress: f32,
    pub reaction_rate: f32,
    pub gas_generation_rate: f32,
    pub bubble_rate: f32,
    pub pressure_impulse: f32,
    pub heat_output: f32,
    pub liquid_turbulence: f32,
    pub precipitate_generation: f32,
    pub colour_transition: f32,
    pub splash_rate: f32,
    pub foam_amount: f32,
    pub flame_rate: f32,
    pub container_vibration: f32,
}

impl ReactionVisualInputs {
    /// Resolves reusable visual controls without inspecting reaction, species,
    /// or chemical names.
    #[must_use]
    pub fn from_effects(
        effects: &[PresentationEffect],
        ordinal: u16,
        ordinal_progress: f32,
        final_ordinal: u16,
    ) -> Self {
        let reaction_progress = if final_ordinal == 0 {
            ordinal_progress.clamp(0.0, 1.0)
        } else {
            ((f32::from(ordinal) + ordinal_progress.clamp(0.0, 1.0))
                / f32::from(final_ordinal.saturating_add(1)))
            .clamp(0.0, 1.0)
        };
        let mut inputs = Self {
            reaction_progress,
            ..Self::default()
        };
        for effect in effects
            .iter()
            .filter(|effect| effect.start_ordinal <= ordinal && ordinal <= effect.end_ordinal)
        {
            let intensity = match effect.intensity {
                EffectIntensity::Subtle => 0.42,
                EffectIntensity::Moderate => 0.70,
                EffectIntensity::Strong => 1.0,
            };
            let span = f32::from(
                effect
                    .end_ordinal
                    .saturating_sub(effect.start_ordinal)
                    .saturating_add(1),
            );
            let elapsed = f32::from(ordinal.saturating_sub(effect.start_ordinal))
                + ordinal_progress.clamp(0.0, 1.0);
            let local_progress = (elapsed / span.max(1.0)).clamp(0.0, 1.0);
            let attack = exponential_response(local_progress / 0.16, 3.8);
            let release = 1.0 - exponential_response((local_progress - 0.76) / 0.24, 3.2);
            let activity = intensity * attack * release;
            match effect.effect {
                EffectProfile::BubbleEmitter => {
                    inputs.bubble_rate += activity;
                    inputs.liquid_turbulence += activity * 0.28;
                }
                EffectProfile::GasRelease => {
                    inputs.gas_generation_rate += activity;
                    inputs.pressure_impulse += activity * 0.18;
                }
                EffectProfile::SurfaceDisturbance => {
                    inputs.liquid_turbulence += activity;
                }
                EffectProfile::LiquidMixing => {
                    inputs.liquid_turbulence += activity * 0.88;
                }
                EffectProfile::SplashEmitter => {
                    inputs.splash_rate += activity;
                    inputs.liquid_turbulence += activity * 0.72;
                    inputs.pressure_impulse += activity * 0.58;
                }
                EffectProfile::PrecipitateFormation | EffectProfile::Clouding => {
                    inputs.precipitate_generation += activity;
                }
                EffectProfile::ColourTransition => inputs.colour_transition += activity,
                EffectProfile::HeatDistortion => inputs.heat_output += activity,
                EffectProfile::FlameEmitter(_) => {
                    inputs.flame_rate += activity;
                    inputs.heat_output += activity * 0.72;
                    inputs.liquid_turbulence += activity * 0.12;
                }
                EffectProfile::ObjectShrinkage => {}
            }
        }
        inputs.gas_generation_rate = inputs.gas_generation_rate.min(1.0);
        inputs.bubble_rate = inputs.bubble_rate.min(1.0);
        inputs.pressure_impulse = inputs.pressure_impulse.min(1.0);
        inputs.heat_output = inputs.heat_output.min(1.0);
        inputs.liquid_turbulence = inputs.liquid_turbulence.min(1.0);
        inputs.precipitate_generation = inputs.precipitate_generation.min(1.0);
        inputs.colour_transition = inputs.colour_transition.min(1.0);
        inputs.splash_rate = inputs.splash_rate.min(1.0);
        inputs.flame_rate = inputs.flame_rate.min(1.0);
        inputs.container_vibration = (inputs.bubble_rate * 0.04
            + inputs.gas_generation_rate * 0.05
            + inputs.pressure_impulse * 0.30
            + inputs.liquid_turbulence * 0.16
            + inputs.splash_rate * 0.25
            + inputs.flame_rate * 0.12)
            .min(0.55);
        inputs.reaction_rate = inputs
            .gas_generation_rate
            .max(inputs.bubble_rate)
            .max(inputs.liquid_turbulence)
            .max(inputs.precipitate_generation)
            .max(inputs.colour_transition)
            .max(inputs.heat_output)
            .max(inputs.flame_rate);
        inputs
    }
}

fn exponential_response(value: f32, rate: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    let denominator = 1.0 - (-rate).exp();
    if denominator.abs() <= f32::EPSILON {
        value
    } else {
        ((1.0 - (-rate * value).exp()) / denominator).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraBehaviour {
    WideEstablishingShot,
    SlowPushIn,
    ReactionFocus,
    ObservationCloseUp,
    SlowPullBack,
    FinalHeroShot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraCue {
    pub behaviour: CameraBehaviour,
    pub start_ordinal: u16,
    pub end_ordinal: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationProfile {
    pub id: String,
    pub environment: AssetProfile,
    pub objects: Vec<PresentationObject>,
    pub effects: Vec<PresentationEffect>,
    pub camera: Vec<CameraCue>,
    pub equation: String,
    pub disclosure: String,
}

/// Chemical role of one catalogue-resolved material in a macroscopic scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroscopicMaterialRole {
    Reactant,
    Product,
}

/// Renderer-independent material fact resolved from a trusted catalogue.
///
/// `phase` is deliberately mandatory here: callers with an older catalogue
/// must use their reviewed legacy profile rather than silently guessing from a
/// name, formula, or representation kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroscopicMaterial {
    pub binding: String,
    pub semantic_identity: String,
    pub role: MacroscopicMaterialRole,
    pub phase: Phase,
    pub representation: RepresentationKind,
}

/// Generic input for phase-driven visual compilation. It contains no reaction
/// identity and therefore cannot select a named-reaction animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroscopicReaction {
    pub profile_id: String,
    pub equation: String,
    pub materials: Vec<MacroscopicMaterial>,
    pub intensity: EffectIntensity,
}

/// Compiles reusable assets and effects from trusted material phases and typed
/// observations. Chemistry remains upstream: this function neither predicts a
/// product nor infers a phase.
///
/// # Errors
///
/// Returns an error when a binding is duplicated, a typed gas observation is
/// not backed by a gaseous catalogue material, or frame ordinals exceed the
/// presentation range.
// Keeping the closed phase/predicate matrix together makes unsupported
// combinations visible and reviewable beside the combinations they exclude.
#[allow(clippy::too_many_lines)]
pub fn compile_phase_driven_profile(
    frames: &SimulationFrames,
    reaction: &MacroscopicReaction,
) -> Result<PresentationProfile, PhaseDrivenProfileError> {
    let final_ordinal = frames
        .frames()
        .last()
        .and_then(|frame| u16::try_from(frame.ordinal()).ok())
        .ok_or(PhaseDrivenProfileError::PresentationRange)?;
    let by_binding = reaction
        .materials
        .iter()
        .map(|material| (material.binding.as_str(), material))
        .collect::<BTreeMap<_, _>>();
    if by_binding.len() != reaction.materials.len() {
        return Err(PhaseDrivenProfileError::DuplicateBinding);
    }

    let transform = |translation, scale| PresentationTransform {
        translation,
        rotation: [0, 0, 0],
        scale,
    };
    let mut objects = vec![PresentationObject {
        id: "vessel".to_owned(),
        asset: AssetProfile::Beaker,
        semantic_identity: "open reaction vessel".to_owned(),
        appearance: AppearanceProfile::ClearGlass,
        role: SceneRole::Vessel,
        transform: transform([0, 0, 0], [1_100, 1_100, 1_100]),
        visible_from_ordinal: 0,
        observation: None,
        colour_transition: None,
    }];
    let has_mobile_reactant = reaction.materials.iter().any(|material| {
        material.role == MacroscopicMaterialRole::Reactant
            && matches!(material.phase, Phase::Aqueous | Phase::Liquid)
    });
    if has_mobile_reactant {
        objects.push(PresentationObject {
            id: "mobile-phase".to_owned(),
            asset: AssetProfile::LiquidVolume,
            semantic_identity: "catalogue-resolved mobile reaction phase".to_owned(),
            appearance: AppearanceProfile::AqueousColourless,
            role: SceneRole::Contents,
            transform: transform([0, -150, 0], [1_000, 850, 1_000]),
            visible_from_ordinal: 0,
            observation: None,
            colour_transition: None,
        });
    }

    let mut reactant_slot = 0_i16;
    for material in reaction
        .materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Reactant)
    {
        let asset = match material.phase {
            Phase::Aqueous | Phase::Liquid => continue,
            Phase::Gas => AssetProfile::GasCloud,
            Phase::Solid if material.representation == RepresentationKind::Metallic => {
                AssetProfile::MetalChunk
            }
            Phase::Solid => AssetProfile::PowderPile,
        };
        let x = if reactant_slot % 2 == 0 { -280 } else { 280 };
        reactant_slot = reactant_slot.saturating_add(1);
        objects.push(PresentationObject {
            id: material.binding.clone(),
            asset,
            semantic_identity: material.semantic_identity.clone(),
            appearance: appearance_for_material(material),
            role: SceneRole::Reactant,
            transform: transform([x, 610, 0], [650, 650, 650]),
            visible_from_ordinal: 0,
            observation: None,
            colour_transition: None,
        });
    }

    let active = active_observations_by_binding(frames)?;
    let mut effects = Vec::new();
    for ((binding, predicate), (ordinal, value)) in &active {
        let Some(material) = by_binding.get(binding.as_str()).copied() else {
            continue;
        };
        match predicate {
            ObservationPredicate::Evolves => {
                if material.phase != Phase::Gas {
                    return Err(PhaseDrivenProfileError::GasObservationPhaseMismatch(
                        binding.clone(),
                    ));
                }
                add_product_object(
                    &mut objects,
                    material,
                    *ordinal,
                    ObservationPredicate::Evolves,
                    None,
                    &transform,
                    has_mobile_reactant,
                );
                push_effect(
                    &mut effects,
                    EffectProfile::GasRelease,
                    *predicate,
                    *ordinal,
                    final_ordinal,
                    reaction.intensity,
                );
                if has_mobile_reactant {
                    push_effect(
                        &mut effects,
                        EffectProfile::BubbleEmitter,
                        *predicate,
                        *ordinal,
                        final_ordinal,
                        reaction.intensity,
                    );
                    push_effect(
                        &mut effects,
                        EffectProfile::SurfaceDisturbance,
                        *predicate,
                        *ordinal,
                        final_ordinal,
                        EffectIntensity::Subtle,
                    );
                }
            }
            ObservationPredicate::Forms => match material.phase {
                Phase::Gas => {
                    add_product_object(
                        &mut objects,
                        material,
                        *ordinal,
                        *predicate,
                        None,
                        &transform,
                        has_mobile_reactant,
                    );
                    push_effect(
                        &mut effects,
                        EffectProfile::GasRelease,
                        *predicate,
                        *ordinal,
                        final_ordinal,
                        reaction.intensity,
                    );
                    if has_mobile_reactant {
                        push_effect(
                            &mut effects,
                            EffectProfile::BubbleEmitter,
                            *predicate,
                            *ordinal,
                            final_ordinal,
                            reaction.intensity,
                        );
                    }
                }
                Phase::Solid => {
                    add_product_object(
                        &mut objects,
                        material,
                        *ordinal,
                        *predicate,
                        None,
                        &transform,
                        has_mobile_reactant,
                    );
                    if has_mobile_reactant {
                        push_effect(
                            &mut effects,
                            EffectProfile::PrecipitateFormation,
                            *predicate,
                            *ordinal,
                            final_ordinal,
                            reaction.intensity,
                        );
                        push_effect(
                            &mut effects,
                            EffectProfile::Clouding,
                            *predicate,
                            *ordinal,
                            final_ordinal,
                            EffectIntensity::Subtle,
                        );
                    }
                }
                Phase::Aqueous | Phase::Liquid => {
                    push_effect(
                        &mut effects,
                        EffectProfile::LiquidMixing,
                        *predicate,
                        *ordinal,
                        final_ordinal,
                        reaction.intensity,
                    );
                    if !has_mobile_reactant {
                        add_product_object(
                            &mut objects,
                            material,
                            *ordinal,
                            *predicate,
                            None,
                            &transform,
                            false,
                        );
                    }
                }
            },
            ObservationPredicate::Disappears => {
                if matches!(material.phase, Phase::Aqueous | Phase::Liquid) {
                    push_effect(
                        &mut effects,
                        EffectProfile::LiquidMixing,
                        *predicate,
                        *ordinal,
                        final_ordinal,
                        reaction.intensity,
                    );
                } else {
                    push_effect(
                        &mut effects,
                        EffectProfile::ObjectShrinkage,
                        *predicate,
                        *ordinal,
                        final_ordinal,
                        reaction.intensity,
                    );
                }
                if has_mobile_reactant {
                    push_effect(
                        &mut effects,
                        EffectProfile::SurfaceDisturbance,
                        *predicate,
                        *ordinal,
                        final_ordinal,
                        EffectIntensity::Subtle,
                    );
                }
            }
            ObservationPredicate::Colour => {
                let value = value
                    .as_ref()
                    .ok_or_else(|| PhaseDrivenProfileError::InvalidColour(binding.clone()))?;
                let target = visual_colour(value)
                    .ok_or_else(|| PhaseDrivenProfileError::InvalidColour(binding.clone()))?;
                if !objects.iter().any(|object| object.id == *binding)
                    && material.role == MacroscopicMaterialRole::Product
                    && !matches!(material.phase, Phase::Aqueous | Phase::Liquid)
                {
                    add_product_object(
                        &mut objects,
                        material,
                        *ordinal,
                        ObservationPredicate::Colour,
                        Some(value.clone()),
                        &transform,
                        has_mobile_reactant,
                    );
                }
                let colour_target = objects
                    .iter()
                    .position(|object| object.id == *binding)
                    .or_else(|| {
                        objects
                            .iter()
                            .position(|object| object.id == "mobile-phase")
                    });
                if let Some(index) = colour_target {
                    let object = &mut objects[index];
                    object.colour_transition = Some(PresentationColourTransition {
                        subject_binding: binding.clone(),
                        value: value.clone(),
                        target,
                        start_ordinal: *ordinal,
                    });
                    push_effect(
                        &mut effects,
                        EffectProfile::ColourTransition,
                        *predicate,
                        *ordinal,
                        final_ordinal,
                        reaction.intensity,
                    );
                }
            }
        }
    }

    Ok(PresentationProfile {
        id: reaction.profile_id.clone(),
        environment: AssetProfile::LaboratoryBench,
        objects,
        effects,
        camera: vec![CameraCue {
            behaviour: CameraBehaviour::WideEstablishingShot,
            start_ordinal: 0,
            end_ordinal: final_ordinal,
        }],
        equation: reaction.equation.clone(),
        disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    })
}

type ActiveObservationKey = (String, ObservationPredicate);
type ActiveObservationValue = (u16, Option<String>);
type ActiveObservations = BTreeMap<ActiveObservationKey, ActiveObservationValue>;

fn active_observations_by_binding(
    frames: &SimulationFrames,
) -> Result<ActiveObservations, PhaseDrivenProfileError> {
    let mut active = BTreeMap::new();
    for frame in frames.frames() {
        let ordinal = u16::try_from(frame.ordinal())
            .map_err(|_| PhaseDrivenProfileError::PresentationRange)?;
        for observation in frame
            .observations()
            .iter()
            .filter(|observation| observation.status == ObservationStatus::Active)
        {
            active
                .entry((observation.subject_binding.clone(), observation.predicate))
                .or_insert_with(|| (ordinal, observation.value.clone()));
        }
    }
    Ok(active)
}

fn appearance_for_material(material: &MacroscopicMaterial) -> AppearanceProfile {
    match (material.phase, material.representation) {
        (Phase::Aqueous | Phase::Liquid | Phase::Gas, _) => AppearanceProfile::AqueousColourless,
        (Phase::Solid, RepresentationKind::Metallic) => AppearanceProfile::MetalSilver,
        (Phase::Solid, _) => AppearanceProfile::LaboratoryNeutral,
    }
}

fn add_product_object<F>(
    objects: &mut Vec<PresentationObject>,
    material: &MacroscopicMaterial,
    ordinal: u16,
    predicate: ObservationPredicate,
    value: Option<String>,
    transform: &F,
    settles_in_liquid: bool,
) where
    F: Fn([i16; 3], [u16; 3]) -> PresentationTransform,
{
    if objects.iter().any(|object| object.id == material.binding) {
        return;
    }
    let (asset, translation, scale) = match material.phase {
        Phase::Gas => (AssetProfile::GasCloud, [160, 930, 0], [620, 620, 620]),
        Phase::Solid if settles_in_liquid => (
            AssetProfile::PrecipitateCloud,
            [0, -520, 0],
            [760, 360, 760],
        ),
        Phase::Solid => (AssetProfile::CrystalCluster, [0, 220, 0], [750, 750, 750]),
        Phase::Aqueous | Phase::Liquid => (
            AssetProfile::LiquidVolume,
            [0, -150, 0],
            [1_000, 850, 1_000],
        ),
    };
    objects.push(PresentationObject {
        id: material.binding.clone(),
        asset,
        semantic_identity: material.semantic_identity.clone(),
        appearance: appearance_for_material(material),
        role: SceneRole::Product,
        transform: transform(translation, scale),
        visible_from_ordinal: ordinal,
        observation: Some(ObjectObservationBinding { predicate, value }),
        colour_transition: None,
    });
}

fn push_effect(
    effects: &mut Vec<PresentationEffect>,
    effect: EffectProfile,
    trigger: ObservationPredicate,
    start_ordinal: u16,
    end_ordinal: u16,
    intensity: EffectIntensity,
) {
    let candidate = PresentationEffect {
        effect,
        trigger,
        intensity,
        start_ordinal,
        end_ordinal,
    };
    if !effects.contains(&candidate) {
        effects.push(candidate);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseDrivenProfileError {
    PresentationRange,
    DuplicateBinding,
    GasObservationPhaseMismatch(String),
    InvalidColour(String),
}

impl fmt::Display for PhaseDrivenProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PresentationRange => {
                formatter.write_str("trusted frames exceed the presentation range")
            }
            Self::DuplicateBinding => {
                formatter.write_str("macroscopic material bindings are not unique")
            }
            Self::GasObservationPhaseMismatch(binding) => write!(
                formatter,
                "gas observation binding `{binding}` is not catalogue-resolved as gas"
            ),
            Self::InvalidColour(binding) => write!(
                formatter,
                "colour observation binding `{binding}` has no supported value"
            ),
        }
    }
}

impl std::error::Error for PhaseDrivenProfileError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroscopicAnnotation {
    pub start_ordinal: u16,
    pub end_ordinal: u16,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealWorldBeat {
    pub start_ordinal: u16,
    pub end_ordinal: u16,
    pub duration_ms: u32,
    pub camera: CameraCue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealWorldTimeline {
    pub beats: Vec<RealWorldBeat>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
            return Some(RealWorldPosition {
                beat_index,
                ordinal,
                ordinal_progress: scaled.fract(),
                beat_progress,
            });
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenePlan {
    pub id: ContentDigest,
    pub reaction: ContentDigest,
    pub profile_id: String,
    pub environment: AssetProfile,
    pub objects: Vec<PresentationObject>,
    pub effects: Vec<PresentationEffect>,
    pub camera: Vec<CameraCue>,
    pub equation: String,
    pub annotations: Vec<MacroscopicAnnotation>,
    pub timeline: RealWorldTimeline,
    pub disclosure: String,
    pub virtual_only_disclosure: String,
}

/// Binds a host-selected visual profile to a trusted generation. Effects and
/// observation-bound objects must begin no earlier than the matching active
/// observation; value-bearing bindings must also match the trusted value.
///
/// # Errors
///
/// Returns an error when a visual precedes or mismatches its validated
/// observation, or the trusted frame digest is unavailable.
pub fn compile_real_world_plan(
    frames: &SimulationFrames,
    profile: &PresentationProfile,
) -> Result<ScenePlan, PlanError> {
    let active_observations = frames
        .frames()
        .iter()
        .filter_map(|frame| {
            u16::try_from(frame.ordinal())
                .ok()
                .map(|ordinal| (ordinal, frame))
        })
        .flat_map(|(ordinal, frame)| {
            frame
                .observations()
                .iter()
                .filter(|observation| observation.status == ObservationStatus::Active)
                .map(move |observation| (ordinal, observation))
        })
        .collect::<Vec<_>>();
    if profile
        .effects
        .iter()
        .any(|effect| !effect_observation_is_compatible(effect.effect, effect.trigger))
    {
        return Err(PlanError::IncompatibleEffectObservation);
    }
    if profile.effects.iter().any(|effect| {
        active_observations
            .iter()
            .filter(|(_, observation)| observation.predicate == effect.trigger)
            .map(|(ordinal, _)| *ordinal)
            .min()
            .is_none_or(|ordinal| effect.start_ordinal < ordinal)
    }) {
        return Err(PlanError::UnsupportedEffectTrigger);
    }
    if profile
        .objects
        .iter()
        .any(|object| !object_observation_is_compatible(object))
    {
        return Err(PlanError::IncompatibleObjectObservation);
    }
    validate_colour_transitions(profile, &active_observations)?;
    if profile.objects.iter().any(|object| {
        (object.role == SceneRole::Product && object.observation.is_none())
            || object.observation.as_ref().is_some_and(|binding| {
                active_observations
                    .iter()
                    .filter(|(_, observation)| {
                        observation.predicate == binding.predicate
                            && observation.value == binding.value
                            && (binding.predicate != ObservationPredicate::Colour
                                || appearance_colour_value(object.appearance)
                                    == binding.value.as_deref())
                    })
                    .map(|(ordinal, _)| *ordinal)
                    .min()
                    .is_none_or(|ordinal| object.visible_from_ordinal < ordinal)
            })
    }) {
        return Err(PlanError::UnsupportedObjectObservation);
    }
    let final_ordinal = frames
        .frames()
        .last()
        .and_then(|frame| u16::try_from(frame.ordinal()).ok())
        .ok_or(PlanError::PresentationRange)?;
    let timeline = compile_real_world_timeline(profile, final_ordinal);
    let annotations = compile_macroscopic_annotations(frames, final_ordinal);
    let reaction = frames.digest().map_err(|_| PlanError::Digest)?;
    Ok(ScenePlan {
        id: reaction,
        reaction,
        profile_id: profile.id.clone(),
        environment: profile.environment,
        objects: profile.objects.clone(),
        effects: profile.effects.clone(),
        camera: profile.camera.clone(),
        equation: profile.equation.clone(),
        annotations,
        timeline,
        disclosure: profile.disclosure.clone(),
        virtual_only_disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    })
}

fn validate_colour_transitions(
    profile: &PresentationProfile,
    active_observations: &[(u16, &FrameObservation)],
) -> Result<(), PlanError> {
    for transition in profile
        .objects
        .iter()
        .filter_map(|object| object.colour_transition.as_ref())
    {
        if visual_colour(&transition.value) != Some(transition.target) {
            return Err(PlanError::InvalidVisualColour);
        }
        let trigger = active_observations
            .iter()
            .filter(|(_, observation)| {
                observation.predicate == ObservationPredicate::Colour
                    && observation.subject_binding == transition.subject_binding
                    && observation.value.as_deref() == Some(transition.value.as_str())
            })
            .map(|(ordinal, _)| *ordinal)
            .min();
        if trigger.is_none_or(|ordinal| transition.start_ordinal < ordinal) {
            return Err(PlanError::UnsupportedColourObservation);
        }
    }
    Ok(())
}

const fn effect_observation_is_compatible(
    effect: EffectProfile,
    predicate: ObservationPredicate,
) -> bool {
    match effect {
        EffectProfile::BubbleEmitter | EffectProfile::GasRelease => matches!(
            predicate,
            ObservationPredicate::Evolves | ObservationPredicate::Forms
        ),
        EffectProfile::FlameEmitter(_) => matches!(predicate, ObservationPredicate::Evolves),
        EffectProfile::ObjectShrinkage => {
            matches!(predicate, ObservationPredicate::Disappears)
        }
        EffectProfile::PrecipitateFormation | EffectProfile::Clouding => {
            matches!(predicate, ObservationPredicate::Forms)
        }
        EffectProfile::ColourTransition => matches!(predicate, ObservationPredicate::Colour),
        EffectProfile::SurfaceDisturbance
        | EffectProfile::LiquidMixing
        | EffectProfile::SplashEmitter => matches!(
            predicate,
            ObservationPredicate::Evolves
                | ObservationPredicate::Disappears
                | ObservationPredicate::Forms
        ),
        // `.chems 1` has no typed thermal observation. Keeping this closed
        // prevents a renderer profile from treating an unrelated observation
        // as proof of heat release.
        EffectProfile::HeatDistortion => false,
    }
}

fn object_observation_is_compatible(object: &PresentationObject) -> bool {
    if object.role != SceneRole::Product {
        return true;
    }
    let Some(binding) = &object.observation else {
        // The existing observation-presence validation reports this separately.
        return true;
    };
    match object.asset {
        AssetProfile::GasCloud => matches!(
            binding.predicate,
            ObservationPredicate::Evolves | ObservationPredicate::Forms
        ),
        AssetProfile::PrecipitateCloud
        | AssetProfile::CrystalCluster
        | AssetProfile::PowderPile => matches!(
            binding.predicate,
            ObservationPredicate::Forms | ObservationPredicate::Colour
        ),
        AssetProfile::LiquidVolume => matches!(binding.predicate, ObservationPredicate::Forms),
        AssetProfile::LaboratoryBench
        | AssetProfile::DarkPresentationPlatform
        | AssetProfile::Beaker
        | AssetProfile::TestTube
        | AssetProfile::ConicalFlask
        | AssetProfile::MeasuringCylinder
        | AssetProfile::MetalChunk
        | AssetProfile::MetalStrip => matches!(
            binding.predicate,
            ObservationPredicate::Forms | ObservationPredicate::Colour
        ),
    }
}

const fn appearance_colour_value(appearance: AppearanceProfile) -> Option<&'static str> {
    match appearance {
        AppearanceProfile::WhitePrecipitate => Some("White"),
        AppearanceProfile::CreamPrecipitate => Some("Cream"),
        AppearanceProfile::YellowPrecipitate => Some("Yellow"),
        _ => None,
    }
}

fn compile_real_world_timeline(
    profile: &PresentationProfile,
    final_ordinal: u16,
) -> RealWorldTimeline {
    let mut boundaries = BTreeSet::from([0, final_ordinal.saturating_add(1)]);
    for object in &profile.objects {
        boundaries.insert(object.visible_from_ordinal);
        if let Some(transition) = &object.colour_transition {
            boundaries.insert(transition.start_ordinal);
        }
    }
    for effect in &profile.effects {
        boundaries.insert(effect.start_ordinal);
        boundaries.insert(effect.end_ordinal.saturating_add(1));
    }
    for cue in &profile.camera {
        boundaries.insert(cue.start_ordinal);
        boundaries.insert(cue.end_ordinal.saturating_add(1));
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
                let intensity = profile
                    .effects
                    .iter()
                    .filter(|effect| {
                        effect.start_ordinal <= start_ordinal && start_ordinal <= effect.end_ordinal
                    })
                    .map(|effect| effect.intensity)
                    .max();
                let duration_ms = macroscopic_beat_duration_ms(
                    intensity,
                    start_ordinal == 0,
                    end_ordinal == final_ordinal,
                );
                let behaviour = profile
                    .camera
                    .iter()
                    .filter(|cue| {
                        cue.start_ordinal <= start_ordinal && start_ordinal <= cue.end_ordinal
                    })
                    .min_by_key(|cue| cue.end_ordinal.saturating_sub(cue.start_ordinal))
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

/// Conservative presentation defaults used when reviewed source does not
/// provide measured kinetics. The entry beat is deliberately close to the
/// duration of a short gravity-driven drop; active effects remain long enough to
/// read while stronger activity resolves more quickly than subtle activity.
const fn macroscopic_beat_duration_ms(
    intensity: Option<EffectIntensity>,
    starts_at_initial_state: bool,
    is_final: bool,
) -> u32 {
    let duration_ms = match intensity {
        Some(EffectIntensity::Strong) => 2_600,
        Some(EffectIntensity::Moderate) => 3_400,
        Some(EffectIntensity::Subtle) => 4_400,
        None if starts_at_initial_state => 900,
        None => 1_800,
    };
    if is_final {
        if duration_ms < 2_400 {
            2_400
        } else {
            duration_ms
        }
    } else {
        duration_ms
    }
}

fn compile_macroscopic_annotations(
    frames: &SimulationFrames,
    final_ordinal: u16,
) -> Vec<MacroscopicAnnotation> {
    let mut annotations = vec![MacroscopicAnnotation {
        start_ordinal: 0,
        end_ordinal: final_ordinal.min(1),
        title: "INITIAL STATE".to_owned(),
        text: "The validated reactants are established in the virtual vessel.".to_owned(),
    }];
    for frame in frames.frames() {
        let Ok(ordinal) = u16::try_from(frame.ordinal()) else {
            continue;
        };
        for observation in frame
            .observations()
            .iter()
            .filter(|observation| observation.status == ObservationStatus::Active)
        {
            annotations.push(MacroscopicAnnotation {
                start_ordinal: ordinal,
                end_ordinal: final_ordinal,
                title: observation_title(observation.predicate).to_owned(),
                text: observation_text(observation.predicate, observation.value.as_deref()),
            });
        }
    }
    annotations.push(MacroscopicAnnotation {
        start_ordinal: final_ordinal,
        end_ordinal: final_ordinal,
        title: "VALIDATED OUTCOME".to_owned(),
        text: "The trusted frame sequence has reached its reviewed outcome.".to_owned(),
    });
    annotations
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    MissingFrames,
    InvalidFrameSequence,
    MissingOperation(u32),
    IncompatibleEffectObservation,
    UnsupportedEffectTrigger,
    IncompatibleObjectObservation,
    UnsupportedObjectObservation,
    InvalidVisualColour,
    UnsupportedColourObservation,
    PresentationRange,
    Digest,
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFrames => formatter.write_str("trusted frames are absent"),
            Self::InvalidFrameSequence => formatter.write_str("frame ordinals are not contiguous"),
            Self::MissingOperation(ordinal) => {
                write!(formatter, "frame {ordinal} has no operation")
            }
            Self::IncompatibleEffectObservation => formatter
                .write_str("presentation effect is incompatible with its typed observation"),
            Self::UnsupportedEffectTrigger => formatter.write_str(
                "presentation effect precedes or lacks an active validated observation trigger",
            ),
            Self::IncompatibleObjectObservation => formatter
                .write_str("presentation object phase is incompatible with its typed observation"),
            Self::UnsupportedObjectObservation => formatter.write_str(
                "presentation object precedes or mismatches its active validated observation",
            ),
            Self::InvalidVisualColour => formatter.write_str(
                "presentation colour is unsupported or mismatches its `.chems` colour value",
            ),
            Self::UnsupportedColourObservation => formatter.write_str(
                "presentation colour precedes or mismatches its active validated colour observation",
            ),
            Self::PresentationRange => {
                formatter.write_str("trusted frames exceed the presentation range")
            }
            Self::Digest => formatter.write_str("trusted frame digest is unavailable"),
        }
    }
}

impl std::error::Error for PlanError {}

#[cfg(test)]
mod tests {
    use chem_catalogue::ObservationPredicate;

    use super::{
        EducationalPlan, EducationalScene, EducationalSceneKind, EffectIntensity, EffectProfile,
        FlamePalette, PresentationEffect, ReactionVisualInputs, TimelinePosition, VisualColour,
        macroscopic_beat_duration_ms, visual_colour,
    };

    #[test]
    fn visual_colours_support_reviewed_names_and_exact_rgb_without_schema_changes() {
        assert_eq!(
            visual_colour("Cream"),
            Some(VisualColour {
                red: 0xf0,
                green: 0xe0,
                blue: 0xad,
            })
        );
        assert_eq!(
            visual_colour("Rgb.Hex12ABEF"),
            Some(VisualColour {
                red: 0x12,
                green: 0xab,
                blue: 0xef,
            })
        );
        assert_eq!(visual_colour("Rgb.Hex12AXEF"), None);
        assert_eq!(visual_colour("UnreviewedMagenta"), None);
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
            scenes,
        }
    }

    #[test]
    fn educational_timeline_locates_boundaries_and_clamps() {
        let plan = timeline_plan(&[1_000, 2_500, 500]);
        assert_eq!(plan.duration_ms(), 4_000);
        assert_eq!(
            plan.locate(1_000),
            Some(TimelinePosition {
                scene_index: 1,
                scene_elapsed_ms: 0,
            })
        );
        assert_eq!(
            plan.locate(u64::MAX),
            Some(TimelinePosition {
                scene_index: 2,
                scene_elapsed_ms: 500,
            })
        );
    }

    #[test]
    fn educational_timeline_round_trips() {
        let plan = timeline_plan(&[1_000, 2_500, 500]);
        for elapsed_ms in [0, 999, 1_000, 3_499, 3_500, 4_000] {
            let position = plan.locate(elapsed_ms).expect("position exists");
            assert_eq!(plan.elapsed_at(position), Some(elapsed_ms));
        }
    }

    #[test]
    fn visual_inputs_are_inferred_from_typed_effects_without_reaction_identity() {
        let effects = vec![
            PresentationEffect {
                effect: EffectProfile::BubbleEmitter,
                trigger: ObservationPredicate::Evolves,
                intensity: EffectIntensity::Moderate,
                start_ordinal: 2,
                end_ordinal: 6,
            },
            PresentationEffect {
                effect: EffectProfile::GasRelease,
                trigger: ObservationPredicate::Evolves,
                intensity: EffectIntensity::Moderate,
                start_ordinal: 2,
                end_ordinal: 6,
            },
        ];
        let inputs = ReactionVisualInputs::from_effects(&effects, 4, 0.5, 8);
        let repeated = ReactionVisualInputs::from_effects(&effects, 4, 0.5, 8);

        assert_eq!(inputs, repeated);
        assert!(inputs.gas_generation_rate > 0.0);
        assert!(inputs.bubble_rate > 0.0);
        assert!(inputs.liquid_turbulence > 0.0);
        assert!(inputs.container_vibration > 0.0);
        assert!(inputs.container_vibration < 0.20);
        assert!(inputs.reaction_rate > 0.0);
        assert!(inputs.foam_amount.abs() < f32::EPSILON);
        assert!(inputs.flame_rate.abs() < f32::EPSILON);
    }

    #[test]
    fn flame_inputs_are_inferred_from_the_generic_typed_effect() {
        let effects = [PresentationEffect {
            effect: EffectProfile::FlameEmitter(FlamePalette::Lilac),
            trigger: ObservationPredicate::Evolves,
            intensity: EffectIntensity::Strong,
            start_ordinal: 2,
            end_ordinal: 6,
        }];
        let inputs = ReactionVisualInputs::from_effects(&effects, 4, 0.5, 8);

        assert!(inputs.flame_rate > 0.9);
        assert!(inputs.container_vibration > 0.10);
        assert!(inputs.heat_output > 0.0);
        assert!(inputs.liquid_turbulence > 0.0);
        assert!((inputs.reaction_rate - inputs.flame_rate).abs() < f32::EPSILON);
    }

    #[test]
    fn liquid_mixing_drives_flow_without_inventing_gas_or_solid_products() {
        let effects = [PresentationEffect {
            effect: EffectProfile::LiquidMixing,
            trigger: ObservationPredicate::Disappears,
            intensity: EffectIntensity::Moderate,
            start_ordinal: 2,
            end_ordinal: 6,
        }];
        let inputs = ReactionVisualInputs::from_effects(&effects, 4, 0.5, 8);

        assert!(inputs.liquid_turbulence > 0.5);
        assert!(inputs.reaction_rate > 0.5);
        assert!(inputs.container_vibration > 0.0);
        assert!(inputs.container_vibration < 0.15);
        assert!(inputs.gas_generation_rate.abs() < f32::EPSILON);
        assert!(inputs.bubble_rate.abs() < f32::EPSILON);
        assert!(inputs.precipitate_generation.abs() < f32::EPSILON);
        assert!(inputs.flame_rate.abs() < f32::EPSILON);
    }

    #[test]
    fn unavailable_visual_properties_use_conservative_zero_defaults() {
        let inputs = ReactionVisualInputs::from_effects(&[], 2, 0.5, 8);
        let completed = ReactionVisualInputs::from_effects(&[], 8, 1.0, 8);

        assert!(inputs.reaction_progress > 0.0);
        assert!((completed.reaction_progress - 1.0).abs() < f32::EPSILON);
        assert!(inputs.reaction_rate.abs() < f32::EPSILON);
        assert!(inputs.gas_generation_rate.abs() < f32::EPSILON);
        assert!(inputs.pressure_impulse.abs() < f32::EPSILON);
        assert!(inputs.heat_output.abs() < f32::EPSILON);
        assert!(inputs.foam_amount.abs() < f32::EPSILON);
        assert!(inputs.flame_rate.abs() < f32::EPSILON);
        assert!(inputs.container_vibration.abs() < f32::EPSILON);
    }

    #[test]
    fn persistent_precipitate_does_not_invent_container_violence() {
        let effects = [PresentationEffect {
            effect: EffectProfile::PrecipitateFormation,
            trigger: ObservationPredicate::Forms,
            intensity: EffectIntensity::Moderate,
            start_ordinal: 2,
            end_ordinal: 6,
        }];
        let inputs = ReactionVisualInputs::from_effects(&effects, 4, 0.5, 8);

        assert!(inputs.precipitate_generation > 0.0);
        assert!(inputs.container_vibration.abs() < f32::EPSILON);
    }

    #[test]
    fn macroscopic_default_timing_is_fast_and_intensity_ordered() {
        let entry = macroscopic_beat_duration_ms(None, true, false);
        let strong = macroscopic_beat_duration_ms(Some(EffectIntensity::Strong), false, false);
        let moderate = macroscopic_beat_duration_ms(Some(EffectIntensity::Moderate), false, false);
        let subtle = macroscopic_beat_duration_ms(Some(EffectIntensity::Subtle), false, false);

        assert_eq!(entry, 900, "a short drop must not become a slow glide");
        assert!(strong < moderate);
        assert!(moderate < subtle);
        assert_eq!(macroscopic_beat_duration_ms(None, false, true), 2_400);
    }
}
