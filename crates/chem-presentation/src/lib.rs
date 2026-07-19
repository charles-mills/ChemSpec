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
    ReactantSetup,
    StructuralChange,
    ObservationConnection,
    Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplanationLabelKind {
    StructuralChangeExplanation,
    ObservationExplanation,
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
pub fn compile_educational_plan(
    frames: &SimulationFrames,
    required_context: &str,
) -> Result<EducationalPlan, PlanError> {
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

    // The playback opens directly on the chemistry: one short scene that
    // establishes the reactant structures, then straight into the
    // operations. Title cards and equation interstitials are gone — the
    // equation already lives in the app header.
    let mut scenes = vec![scene(
        EducationalSceneKind::ReactantSetup,
        first,
        first,
        1_600,
        vec![
            EducationalCue::PreserveDisclosure,
            EducationalCue::EstablishFrame {
                frame: first.trace().state_digest,
            },
        ],
    )];

    let mut transition_index = 1;
    while transition_index < sequence.len() {
        let group_start = transition_index;
        let before = &sequence[group_start - 1];
        let first_after = &sequence[group_start];
        let first_operation = first_after
            .active_operation()
            .ok_or(PlanError::MissingOperation(first_after.ordinal()))?;
        let signature = operation_signature(before, first_after, first_operation.operation.view());
        let first_narration = operation_narration(
            before,
            first_after,
            first_operation.operation.view(),
            required_context,
        );
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
                required_context,
            );
            let candidate_atoms = candidate_narration
                .explanation
                .target_atoms
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            let same_operation = operation_signature(
                candidate_before,
                candidate_after,
                candidate_operation.operation.view(),
            ) == signature;
            let independent_atoms = affected.is_disjoint(&candidate_atoms);
            let supports_shared_center =
                supports_overlapping_group(first_operation.operation.view())
                    && supports_overlapping_group(candidate_operation.operation.view());
            if !same_operation || (!independent_atoms && !supports_shared_center) {
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
        let mut narration = first_narration;
        let operation_count = group_end - group_start + 1;
        if operation_count > 1 {
            let sites = if operation_count == 2 {
                "The same change happens in two places at once.".to_owned()
            } else {
                format!("The same change happens at {operation_count} places at once.")
            };
            narration.explanation.text = format!("{} {sites}", narration.explanation.text);
        }
        let before_digest = before.trace().state_digest;
        let grouped_pacing = u32::try_from(operation_count.saturating_sub(1))
            .unwrap_or(u32::MAX)
            .saturating_mul(140)
            .min(1_200);
        // `explanation_duration` already includes the structural-action lead
        // in. Adding another fixed action duration here made every operation
        // scene pay for that phase twice.
        let duration_ms =
            explanation_duration(&narration.explanation.text).saturating_add(grouped_pacing);
        let representative_atoms = narration.explanation.target_atoms.clone();
        let operations = (group_start..=group_end)
            .map(|index| {
                let operation_before = &sequence[index - 1];
                let operation_after = &sequence[index];
                let active = operation_after
                    .active_operation()
                    .ok_or(PlanError::MissingOperation(operation_after.ordinal()))?;
                let narration = operation_narration(
                    operation_before,
                    operation_after,
                    active.operation.view(),
                    required_context,
                );
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
        if let Some(observed) = observation_scene(after, &representative_atoms) {
            scenes.push(observed);
        }

        transition_index = group_end + 1;
    }

    // The summary recaps every observation the reaction established, so the
    // macroscopic story stays on screen alongside the final structures.
    let mut summary_cues = vec![EducationalCue::EstablishFrame {
        frame: last.trace().state_digest,
    }];
    summary_cues.extend(last.observations().iter().map(|observation| {
        EducationalCue::ShowContext {
            label: ContextLabel {
                kind: ExplanationLabelKind::ObservationExplanation,
                title: observation_title(observation.predicate).to_owned(),
                text: observation_summary(observation.predicate, observation.value.as_deref()),
                target_atoms: Vec::new(),
                connector: false,
            },
        }
    }));
    scenes.push(scene(
        EducationalSceneKind::Summary,
        last,
        last,
        4_200,
        summary_cues,
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

/// A dedicated beat connecting a freshly activated observation to the atoms
/// whose validated change triggered it: the moment the structural story
/// meets what a learner would actually see at the bench. The card anchors
/// to one representative instance; a disappearance has nothing left on the
/// canvas to point at, so its card stands alone.
fn observation_scene(
    frame: &SimulationFrame,
    representative_atoms: &[String],
) -> Option<EducationalScene> {
    let observed = frame
        .observations()
        .iter()
        .filter(|observation| observation.status == ObservationStatus::Active)
        .collect::<Vec<_>>();
    let first = observed.first()?;
    let digest = frame.trace().state_digest;
    let text = observation_text(first.predicate, first.value.as_deref());
    let duration_ms = explanation_duration(&text);
    let anchored = first.predicate != ObservationPredicate::Disappears;
    let mut cues = vec![EducationalCue::EstablishFrame { frame: digest }];
    cues.extend(
        observed
            .iter()
            .map(|observation| EducationalCue::ShowObservation {
                predicate: observation.predicate,
                frame: digest,
            }),
    );
    cues.push(EducationalCue::ShowExplanation {
        label: ExplanationLabel {
            kind: ExplanationLabelKind::ObservationExplanation,
            text,
            target_atoms: if anchored {
                representative_atoms.to_vec()
            } else {
                Vec::new()
            },
            connector: anchored,
        },
    });
    Some(scene(
        EducationalSceneKind::ObservationConnection,
        frame,
        frame,
        duration_ms,
        cues,
    ))
}

/// Sequential covalent changes can share a central atom (for example every
/// I–F bond in IF₇) while still representing one repeated teaching idea. The
/// exact frame boundaries remain in `EducationalOperation`; this only permits
/// the presentation layer to show those validated transitions in one scene.
const fn supports_overlapping_group(operation: StructuralOperationView<'_>) -> bool {
    matches!(
        operation,
        StructuralOperationView::CleaveCovalent { .. }
            | StructuralOperationView::FormCovalent { .. }
    )
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
            allocation,
            ..
        } => format!(
            "cleave:{}:{}:{}:{}",
            atom_symbol(before, after, left),
            atom_symbol(before, after, right),
            expected_order.order(),
            allocation_signature(before, after, allocation),
        ),
        StructuralOperationView::FormCovalent {
            left, right, order, ..
        } => format!(
            "form:{}:{}:{}",
            atom_symbol(before, after, left),
            atom_symbol(before, after, right),
            order.order(),
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

fn allocation_signature(
    before: &SimulationFrame,
    after: &SimulationFrame,
    allocation: &chem_domain::ElectronAllocation,
) -> String {
    match allocation {
        chem_domain::ElectronAllocation::Homolytic => "homolytic".to_owned(),
        chem_domain::ElectronAllocation::HeterolyticTo(atom) => {
            format!("heterolytic-to-{}", atom_symbol(before, after, atom))
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
    required_context: &str,
) -> OperationNarration {
    let (context, explanation, target_atoms) = match operation {
        StructuralOperationView::ReconfigureElectrons { transition } => (
            format!(
                "{} electrons re-pair",
                atom_symbol(before, after, transition.atom())
            ),
            "This atom's own electrons change how they pair up, getting ready for the next bond change."
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
                "{}–{} {} bond breaks",
                atom_symbol(before, after, left),
                atom_symbol(before, after, right),
                bond_order_name(expected_order.order())
            ),
            "The electrons these atoms were sharing pull out of the bond, so the atoms let go of each other."
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
            "The two atoms now share a pair of electrons — that shared pair is the new covalent bond holding them together."
                .to_owned(),
            atom_targets([left, right]),
        ),
        StructuralOperationView::CleaveDative {
            donor, acceptor, ..
        } => (
            format!(
                "{} → {} coordinate bond breaks",
                atom_symbol(before, after, donor),
                atom_symbol(before, after, acceptor)
            ),
            "The coordinate bond breaks apart, and the shared pair of electrons stays with the atom that donated it."
                .to_owned(),
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
            "One atom donates both electrons of the shared pair — that one-sided sharing is called a coordinate bond."
                .to_owned(),
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
            "These atoms change how many electron pairs they share — the more pairs shared, the stronger the bond."
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
                "{}–{} effective bond order: {} → {}",
                atom_symbol(before, after, left),
                atom_symbol(before, after, right),
                delocalization_name(expected),
                delocalization_name(replacement),
            ),
            "These bonding electrons spread out across neighbouring bonds instead of staying between one pair of atoms."
                .to_owned(),
            atom_targets([left, right]),
        ),
        StructuralOperationView::AssociateIonic { association } => {
            let target_atoms = ionic_targets(after, association.id());
            (
                count_phrase(association.components().len(), "ion", "attract"),
                "Opposite charges attract: these ions pull together and sit side by side without sharing any electrons."
                    .to_owned(),
                target_atoms,
            )
        }
        StructuralOperationView::DissociateIonic { association } => {
            let target_atoms = ionic_targets(before, association);
            (
                "Ions drift apart".to_owned(),
                "These ions stop holding on to each other and drift apart, each keeping its own charge."
                    .to_owned(),
                target_atoms,
            )
        }
        StructuralOperationView::ReleaseMetallic { site, .. } => (
            format!(
                "{} leaves the electron sea",
                atom_symbol(before, after, site)
            ),
            "This atom leaves the metal's shared sea of electrons and strikes out on its own."
                .to_owned(),
            atom_targets([site]),
        ),
        StructuralOperationView::JoinMetallic { site, .. } => (
            format!(
                "{} joins the electron sea",
                atom_symbol(before, after, site)
            ),
            "This atom joins the metal's shared sea of electrons, adding its own electrons to the pool."
                .to_owned(),
            atom_targets([site]),
        ),
        StructuralOperationView::TransferElectron {
            donor,
            acceptor,
            count,
            ..
        } => {
            let donor_symbol = atom_symbol(before, after, donor);
            let acceptor_symbol = atom_symbol(before, after, acceptor);
            let electrons = plural(count.into(), "electron");
            if required_context.eq_ignore_ascii_case("electricity") {
                let (context, explanation) = electrolysis_transfer_text(
                    &donor_symbol,
                    &acceptor_symbol,
                    count,
                    &electrons,
                );
                (context, explanation, atom_targets([donor, acceptor]))
            } else {
                (
                    format!("{donor_symbol} → {acceptor_symbol} · {count} {electrons}"),
                    if count == 1 {
                        "An electron jumps from one atom to the other: the giver becomes more positive and the receiver more negative."
                            .to_owned()
                    } else {
                        "Electrons jump from one atom to the other: the giver becomes more positive and the receiver more negative."
                            .to_owned()
                    },
                    atom_targets([donor, acceptor]),
                )
            }
        }
        StructuralOperationView::AssignProduct { atoms, .. } => (
            count_phrase(atoms.len(), "atom", "regrouped"),
            "These atoms now make up a finished product — every atom from the reactants is still here, just regrouped."
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
            title: if required_context.eq_ignore_ascii_case("electricity")
                && matches!(operation, StructuralOperationView::TransferElectron { .. })
            {
                "ELECTROLYSIS · ELECTRON FLOW".to_owned()
            } else {
                operation_title(operation).to_owned()
            },
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

fn electrolysis_transfer_text(
    donor: &str,
    acceptor: &str,
    count: u8,
    electrons: &str,
) -> (String, String) {
    (
        format!("Anode: {donor} transfers {count} {electrons}"),
        format!(
            "Cathode: {acceptor} receives {count} {electrons}. Oxidation occurs at the anode and reduction at the cathode; electrons travel through the external circuit, not directly between these ions."
        ),
    )
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
            "These molecules are a gas: in a real experiment you would see them bubble out of the mixture."
                .to_owned()
        }
        ObservationPredicate::Disappears => {
            "This reactant is being used up: in a real experiment you would watch it shrink away."
                .to_owned()
        }
        ObservationPredicate::Forms => {
            "A new substance has appeared: this grouping is the product you would see form in a real experiment."
                .to_owned()
        }
        ObservationPredicate::Colour => value.map_or_else(
            || "You would see the mixture change colour as this product forms.".to_owned(),
            |colour| format!("You would see the mixture turn {colour} as this product forms."),
        ),
    }
}

/// One-line recap of an observation for the summary chips.
fn observation_summary(predicate: ObservationPredicate, value: Option<&str>) -> String {
    match predicate {
        ObservationPredicate::Evolves => "Gas bubbles out of the mixture".to_owned(),
        ObservationPredicate::Disappears => "A reactant is used up".to_owned(),
        ObservationPredicate::Forms => "A new product appears".to_owned(),
        ObservationPredicate::Colour => value.map_or_else(
            || "The mixture changes colour".to_owned(),
            |colour| format!("The mixture turns {colour}"),
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
    (2_200_u32.saturating_add(words.saturating_mul(170))).clamp(3_200, 5_600)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetProfile {
    LaboratoryBench,
    DarkPresentationPlatform,
    /// A modular, authored vessel-and-contents clip selected by typed
    /// macroscopic presentation metadata. The renderer may style or suppress
    /// its reusable effect modules, but it must not reinterpret chemistry.
    ReactiveMetalWaterAssembly,
    /// Authored high-energy water-contact assembly. Chemistry selects this
    /// only from a reviewed physical-behaviour fact plus an exact validated
    /// solid-metal/liquid-water/aqueous-ion/gas layout.
    ExplosiveMetalWaterAssembly,
    /// Authored stirring, heating, evaporation, and crystallization modules
    /// for a validated neutralisation with the generic solvent-separation
    /// presentation process.
    NeutralisationEvaporationAssembly,
    /// Authored liquid-fuel, ignition, blue flame, and pale product-plume
    /// modules for validated complete combustion.
    CompleteCombustionAssembly,
    /// Authored liquid-fuel, yellow flame, smoke, and soot modules for
    /// validated combustion whose exact products include carbon monoxide.
    IncompleteCombustionAssembly,
    /// Authored pouring, mixing, clouding, settling-fragment, and persistent
    /// sediment modules. Presentation selects this only after a validated
    /// solid product formation is also authorized as precipitation in a
    /// mobile phase.
    AqueousPrecipitationAssembly,
    /// Authored solution transition, metal erosion, surface deposition, and
    /// detached-flake modules for a validated metal-displacement process.
    MetalDisplacementAssembly,
    /// Authored granular reactants, mixing tool, optional warm reaction front,
    /// ceramic vessel, and persistent solid product.
    SolidSolidSynthesisAssembly,
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
    /// Evidence-backed or chemistry-derived bulk colour. Alpha remains a
    /// property of the phase-specific renderer rather than this RGB value.
    ReviewedColour(VisualColour),
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

/// A validated observation that must activate before an object may be shown.
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

/// One exact trusted material binding and the RGB selected for its authored
/// precipitation material slot. Opacity remains renderer-owned and
/// phase-specific.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundVisualColour {
    pub binding: String,
    /// Reviewed catalogue RGB or conservative phase fallback before an exact
    /// value-bearing observation activates.
    pub base_colour: VisualColour,
    /// Final RGB. When `transition_ordinal` is present this is the exact
    /// validated `.chems` colour observation.
    pub colour: VisualColour,
    pub transition_ordinal: Option<u16>,
}

/// Renderer-independent bindings for the reusable precipitation assembly.
///
/// The formation ordinal is copied from the exact validated `forms`
/// observation that authorized both precipitation and clouding. It is also
/// the absolute-playhead origin for the six-second authored clip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecipitationVisualProfile {
    pub formation_ordinal: u16,
    pub initial_liquid: BoundVisualColour,
    pub added_liquid: BoundVisualColour,
    pub precipitate: BoundVisualColour,
}

/// Authored macroscopic layout selected only from catalogue-resolved reactant
/// phases. Missing, extra, or unsupported phase combinations retain the
/// ordinary reusable-effect renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GasEvolutionVariant {
    LiquidLiquid,
    SolidLiquid,
}

/// Renderer-independent bindings for the reusable gas-evolution clips.
///
/// `generation_ordinal` is the exact validated gas `evolves`/`forms`
/// observation. It gates the bubble/plume part of the authored clip while the
/// earlier addition motion remains ordinary scene setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GasEvolutionVisualProfile {
    pub generation_ordinal: u16,
    pub variant: GasEvolutionVariant,
    pub initial_reactant: BoundVisualColour,
    pub added_reactant: BoundVisualColour,
    pub gas_product: BoundVisualColour,
}

/// Exact validated material bindings for the reusable metal-displacement clip.
///
/// The chemistry layer has already established the cross-side metal identity
/// exchange. Presentation owns only colour resolution and deterministic
/// authored playback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetalDisplacementVisualProfile {
    pub formation_ordinal: u16,
    pub initial_solution: BoundVisualColour,
    pub final_solution: BoundVisualColour,
    pub original_metal: BoundVisualColour,
    pub deposited_metal: BoundVisualColour,
}

/// Exact validated material bindings for the reusable solid-solid synthesis
/// clip. The optional reaction-front cue is presentation-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolidSolidSynthesisVisualProfile {
    pub formation_ordinal: u16,
    pub reactant_a: BoundVisualColour,
    pub reactant_b: BoundVisualColour,
    pub product: BoundVisualColour,
    pub show_reaction_front: bool,
}

/// Exact validated material bindings for the high-energy metal/water clips.
/// The contact ordinal remains tied to validated reaction progression while
/// the authored clip retains its complete absolute six-second timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplosiveMetalWaterVisualProfile {
    pub contact_ordinal: u16,
    pub variant: ExplosiveMetalWaterVariant,
    pub water_reactant: BoundVisualColour,
    pub metal_reactant: BoundVisualColour,
    pub hydroxide_product: BoundVisualColour,
    pub hydrogen_product: BoundVisualColour,
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
    /// Phase-neutral secondary motion tied to validated reaction progression.
    ReactionActivity,
    BubbleEmitter,
    GasRelease,
    VapourRelease,
    SurfaceDisturbance,
    LiquidMixing,
    ObjectShrinkage,
    /// Progressive oxide-layer coverage on an exposed solid metal. The
    /// chemistry layer must authorize this with `SurfaceOxidation`.
    SurfaceOxidation,
    /// Dry or otherwise non-precipitating solid nucleation/growth.
    SolidFormation,
    PrecipitateFormation,
    Clouding,
    ColourTransition,
    SplashEmitter,
    HeatDistortion,
    FlameEmitter(FlamePalette),
}

/// Upstream authority for a reusable macroscopic effect.
///
/// Observation authorization remains the normal `.chems` route. A typed
/// process is available for deterministic chemistry classifications whose
/// validated dynamic frames do not carry repository evidence observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectAuthorization {
    Observation(ObservationPredicate),
    Process(MacroscopicProcess),
}

/// Reviewed flame-colour families available to the generic flame renderer.
///
/// Selecting a palette does not assert that a reaction ignites. A trusted
/// presentation profile must still authorize `FlameEmitter` from either a
/// compatible observation or a closed upstream chemistry process before the
/// renderer can display it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlamePalette {
    Natural,
    BurnerBlue,
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
    pub authorization: EffectAuthorization,
    pub intensity: EffectIntensity,
    pub start_ordinal: u16,
    pub end_ordinal: u16,
    /// Optional exact oxide coating colour. It is meaningful only for a
    /// process-authorized `SurfaceOxidation` effect.
    pub surface_oxide_colour: Option<SurfaceOxideColour>,
}

/// Continuous, renderer-independent macroscopic controls compiled from the
/// currently active, observation- or process-authorized presentation effects.
///
/// Values are normalized illustrative intensities in `0.0..=1.0`, not measured
/// kinetic, thermodynamic, or pressure quantities. Missing reviewed metadata
/// deliberately remains zero instead of being inferred from a chemical name.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReactionVisualInputs {
    pub reaction_progress: f32,
    pub reaction_rate: f32,
    pub gas_generation_rate: f32,
    pub vapour_generation_rate: f32,
    pub bubble_rate: f32,
    pub pressure_impulse: f32,
    pub heat_output: f32,
    pub liquid_turbulence: f32,
    pub solid_formation_rate: f32,
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
            apply_effect_inputs(
                &mut inputs,
                effect.effect,
                effect_activity(effect, ordinal, ordinal_progress),
            );
        }
        inputs.clamp_and_derive();
        inputs
    }

    fn clamp_and_derive(&mut self) {
        self.gas_generation_rate = self.gas_generation_rate.min(1.0);
        self.vapour_generation_rate = self.vapour_generation_rate.min(1.0);
        self.bubble_rate = self.bubble_rate.min(1.0);
        self.pressure_impulse = self.pressure_impulse.min(1.0);
        self.heat_output = self.heat_output.min(1.0);
        self.liquid_turbulence = self.liquid_turbulence.min(1.0);
        self.solid_formation_rate = self.solid_formation_rate.min(1.0);
        self.precipitate_generation = self.precipitate_generation.min(1.0);
        self.colour_transition = self.colour_transition.min(1.0);
        self.splash_rate = self.splash_rate.min(1.0);
        self.flame_rate = self.flame_rate.min(1.0);
        self.container_vibration = (self.bubble_rate * 0.04
            + self.gas_generation_rate * 0.05
            + self.pressure_impulse * 0.30
            + self.liquid_turbulence * 0.16
            + self.solid_formation_rate * 0.05
            + self.splash_rate * 0.25
            + self.flame_rate * 0.12)
            .min(0.55);
        self.reaction_rate = self
            .gas_generation_rate
            .max(self.vapour_generation_rate)
            .max(self.bubble_rate)
            .max(self.liquid_turbulence)
            .max(self.solid_formation_rate)
            .max(self.precipitate_generation)
            .max(self.colour_transition)
            .max(self.heat_output)
            .max(self.flame_rate);
    }
}

fn effect_activity(effect: &PresentationEffect, ordinal: u16, ordinal_progress: f32) -> f32 {
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
    let elapsed =
        f32::from(ordinal.saturating_sub(effect.start_ordinal)) + ordinal_progress.clamp(0.0, 1.0);
    let local_progress = (elapsed / span.max(1.0)).clamp(0.0, 1.0);
    let attack = exponential_response(local_progress / 0.16, 3.8);
    let release = 1.0 - exponential_response((local_progress - 0.76) / 0.24, 3.2);
    intensity * attack * release
}

fn apply_effect_inputs(inputs: &mut ReactionVisualInputs, effect: EffectProfile, activity: f32) {
    match effect {
        EffectProfile::ReactionActivity => {
            inputs.liquid_turbulence += activity * 0.34;
            inputs.pressure_impulse += activity * 0.05;
        }
        EffectProfile::BubbleEmitter => {
            inputs.bubble_rate += activity;
            inputs.liquid_turbulence += activity * 0.28;
        }
        EffectProfile::GasRelease => {
            inputs.gas_generation_rate += activity;
            inputs.pressure_impulse += activity * 0.18;
        }
        EffectProfile::VapourRelease => {
            inputs.vapour_generation_rate += activity;
            inputs.gas_generation_rate += activity * 0.72;
            inputs.heat_output += activity * 0.58;
            inputs.pressure_impulse += activity * 0.10;
        }
        EffectProfile::SurfaceDisturbance => inputs.liquid_turbulence += activity,
        EffectProfile::LiquidMixing => inputs.liquid_turbulence += activity * 0.88,
        EffectProfile::SplashEmitter => {
            inputs.splash_rate += activity;
            inputs.liquid_turbulence += activity * 0.72;
            inputs.pressure_impulse += activity * 0.58;
        }
        EffectProfile::SolidFormation => inputs.solid_formation_rate += activity,
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
        EffectProfile::SurfaceOxidation => {
            inputs.solid_formation_rate += activity * 0.72;
            inputs.pressure_impulse += activity * 0.025;
        }
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
    /// Exact material bindings for an authorized authored precipitation
    /// assembly. `None` prevents the renderer from selecting that assembly.
    pub precipitation: Option<PrecipitationVisualProfile>,
    /// Exact phase and material bindings for a non-combustion authored
    /// gas-evolution layout. `None` keeps the existing fallback animation.
    pub gas_evolution: Option<GasEvolutionVisualProfile>,
    /// Exact cross-side material bindings for an authorized metal-displacement
    /// assembly. `None` keeps the ordinary reusable-effect renderer.
    pub metal_displacement: Option<MetalDisplacementVisualProfile>,
    /// Exact solid reactant and product bindings for an authorized generic
    /// combination assembly.
    pub solid_solid_synthesis: Option<SolidSolidSynthesisVisualProfile>,
    /// Exact bindings and reviewed authored variant for the high-energy
    /// metal/water category.
    pub explosive_metal_water: Option<ExplosiveMetalWaterVisualProfile>,
    /// Optional deterministic physical separation shown only after the
    /// validated reaction state has completed.
    pub post_process: Option<MacroscopicProcess>,
    pub equation: String,
    pub disclosure: String,
}

/// Chemical role of one catalogue-resolved material in a macroscopic scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroscopicMaterialRole {
    Reactant,
    Product,
}

/// Renderer-independent material fact resolved from reference data.
///
/// `phase` is deliberately mandatory here: callers with an older catalogue
/// must use their reviewed legacy profile rather than silently guessing from a
/// name, formula, or representation kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroscopicMaterial {
    pub binding: String,
    pub semantic_identity: String,
    /// Exact validated identity retained for optional, identity-bound
    /// appearance enrichment. Presentation never parses these strings to
    /// decide chemistry.
    pub structure_id: String,
    pub formula: String,
    pub role: MacroscopicMaterialRole,
    pub phase: Phase,
    pub representation: RepresentationKind,
    /// Optional reviewed bulk colour. A `.chems` colour observation remains
    /// higher authority and may animate away from this conservative default.
    pub colour: Option<VisualColour>,
    /// Reviewed water-contact capability carried from the catalogue-aware
    /// chemistry adapter. Renderer code never derives this from strings.
    pub explosive_water_contact: Option<ExplosiveMetalWaterVariant>,
}

/// Generic input for phase-driven visual compilation. It contains no reaction
/// identity and therefore cannot select a named-reaction animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroscopicReaction {
    pub profile_id: String,
    pub equation: String,
    pub materials: Vec<MacroscopicMaterial>,
    pub intensity: EffectIntensity,
    pub process: Option<MacroscopicProcess>,
    /// Exact carbon count of the validated C/H(/O) fuel when `process` is a
    /// combustion classification. It is chemistry-owned visual input, not a
    /// name or formula parsed by presentation.
    pub fuel_carbon_count: Option<u64>,
    /// Optional representative coating colour for the exact validated oxide
    /// product. Reviewed catalogue colour always outranks this value.
    pub surface_oxide_colour: Option<SurfaceOxideColour>,
}

/// Authority retained alongside a macroscopic colour so runtime enrichment
/// cannot be presented as reviewed catalogue chemistry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroscopicColourAuthority {
    Reviewed,
    ModelAsserted,
}

/// Representative oxide coating colour bound to one product binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceOxideColour {
    pub product_binding: String,
    pub target: VisualColour,
    pub authority: MacroscopicColourAuthority,
}

/// A process classification produced by chemistry, never inferred by the
/// renderer from reaction or species names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroscopicProcess {
    /// Two structurally validated soluble ionic reactants produce one exact
    /// solid product in an aqueous mobile phase.
    AqueousPrecipitation,
    /// Two validated mobile reactants generate one exact gaseous product.
    GasEvolutionLiquidLiquid,
    /// One validated solid and one mobile reactant generate one exact gaseous
    /// product.
    GasEvolutionSolidLiquid,
    /// A solid metal becomes the cation in an aqueous ionic product while the
    /// solution's original metal cation becomes a different solid metal.
    MetalDisplacement,
    /// A reviewed extreme-water-contact capability on the exact metallic
    /// reactant, with an exact solid-metal/liquid-water/aqueous-ion/gas
    /// validated layout. The variant selects authored geometry.
    ExplosiveMetalWater(ExplosiveMetalWaterVariant),
    /// Exactly two solid reactants combine into one solid chemical product.
    SolidSolidSynthesis,
    CompleteCombustion,
    /// Validated combustion whose exact gaseous products include carbon
    /// monoxide. The renderer does not infer this from a species name.
    IncompleteCombustion,
    /// Evaporate a validated liquid solvent and grow its already-validated
    /// dissolved ionic product as a crystal residue.
    SolventEvaporationCrystallization,
    /// Exposed solid metal plus gaseous dioxygen producing a validated solid
    /// oxide-family product. This classification is established upstream.
    SurfaceOxidation,
}

/// Typed authored variants for the reusable high-energy metal/water category.
/// They are emitted only from reviewed catalogue facts; their names are not
/// used as renderer-side chemistry selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplosiveMetalWaterVariant {
    Rubidium,
    Caesium,
    Francium,
}

/// Conservative stylised sRGB palette for hydrocarbon fuels, selected from
/// exact validated carbon count. These ranges intentionally remain a visual
/// convention rather than a claim about a specific compound's purity or
/// physical state.
#[must_use]
pub const fn hydrocarbon_fuel_colour(carbon_count: u64) -> VisualColour {
    let [red, green, blue] = match carbon_count {
        0..=4 => [0xee, 0xef, 0xe8],
        5..=8 => [0xe5, 0xd4, 0x82],
        9..=12 => [0xc7, 0x86, 0x32],
        13..=16 => [0x8c, 0x55, 0x2d],
        _ => [0x4f, 0x2d, 0x1d],
    };
    VisualColour { red, green, blue }
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
    let surface_oxidation = reaction.process == Some(MacroscopicProcess::SurfaceOxidation);
    let explosive_metal_water = matches!(
        reaction.process,
        Some(MacroscopicProcess::ExplosiveMetalWater(_))
    );
    let reviewed_surface_oxide_colour = surface_oxidation
        .then(|| {
            reaction
                .materials
                .iter()
                .find(|material| {
                    material.role == MacroscopicMaterialRole::Product && material.colour.is_some()
                })
                .and_then(|material| {
                    material.colour.map(|target| SurfaceOxideColour {
                        product_binding: material.binding.clone(),
                        target,
                        authority: MacroscopicColourAuthority::Reviewed,
                    })
                })
        })
        .flatten();
    let surface_oxide_colour =
        reviewed_surface_oxide_colour.or_else(|| reaction.surface_oxide_colour.clone());
    if let Some(colour) = &surface_oxide_colour
        && (!surface_oxidation
            || !reaction.materials.iter().any(|material| {
                material.role == MacroscopicMaterialRole::Product
                    && material.binding == colour.product_binding
            }))
    {
        return Err(PhaseDrivenProfileError::InvalidSurfaceOxideColourBinding);
    }
    let neutralisation_assembly =
        reaction.process == Some(MacroscopicProcess::SolventEvaporationCrystallization);
    let combustion_asset = match reaction.process {
        Some(MacroscopicProcess::CompleteCombustion) => {
            Some(AssetProfile::CompleteCombustionAssembly)
        }
        Some(MacroscopicProcess::IncompleteCombustion) => {
            Some(AssetProfile::IncompleteCombustionAssembly)
        }
        Some(
            MacroscopicProcess::AqueousPrecipitation
            | MacroscopicProcess::GasEvolutionLiquidLiquid
            | MacroscopicProcess::GasEvolutionSolidLiquid
            | MacroscopicProcess::MetalDisplacement
            | MacroscopicProcess::ExplosiveMetalWater(_)
            | MacroscopicProcess::SolidSolidSynthesis
            | MacroscopicProcess::SolventEvaporationCrystallization
            | MacroscopicProcess::SurfaceOxidation,
        )
        | None => None,
    };
    let mut objects = if surface_oxidation {
        Vec::new()
    } else {
        vec![PresentationObject {
            id: "vessel".to_owned(),
            asset: combustion_asset.unwrap_or(if neutralisation_assembly {
                AssetProfile::NeutralisationEvaporationAssembly
            } else if explosive_metal_water {
                AssetProfile::ExplosiveMetalWaterAssembly
            } else {
                AssetProfile::Beaker
            }),
            semantic_identity: "open reaction vessel".to_owned(),
            appearance: combustion_asset.map_or(AppearanceProfile::ClearGlass, |_| {
                AppearanceProfile::ReviewedColour(hydrocarbon_fuel_colour(
                    reaction.fuel_carbon_count.unwrap_or(1),
                ))
            }),
            role: SceneRole::Vessel,
            transform: transform([0, 0, 0], [1_100, 1_100, 1_100]),
            visible_from_ordinal: 0,
            observation: None,
            colour_transition: None,
        }]
    };
    let has_mobile_reactant = neutralisation_assembly
        || reaction.materials.iter().any(|material| {
            material.role == MacroscopicMaterialRole::Reactant
                && matches!(material.phase, Phase::Aqueous | Phase::Liquid)
        });
    if has_mobile_reactant && combustion_asset.is_none() {
        let mobile_appearance = neutralisation_assembly
            .then(|| {
                reaction
                    .materials
                    .iter()
                    .find(|material| {
                        material.role == MacroscopicMaterialRole::Product
                            && matches!(material.phase, Phase::Aqueous | Phase::Liquid)
                            && material.colour.is_some()
                    })
                    .and_then(|material| material.colour)
                    .map(AppearanceProfile::ReviewedColour)
            })
            .flatten()
            .unwrap_or(AppearanceProfile::AqueousColourless);
        objects.push(PresentationObject {
            id: "mobile-phase".to_owned(),
            asset: AssetProfile::LiquidVolume,
            semantic_identity: "catalogue-resolved mobile reaction phase".to_owned(),
            appearance: mobile_appearance,
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
        if surface_oxidation
            && !(matches!(material.phase, Phase::Solid | Phase::Unknown)
                && material.representation == RepresentationKind::Metallic)
        {
            continue;
        }
        let asset = match material.phase {
            // Reviewed catalogue records always carry a real phase; an
            // unknown phase must not invent bench geometry.
            Phase::Unknown if surface_oxidation => AssetProfile::MetalChunk,
            Phase::Aqueous | Phase::Liquid | Phase::Unknown => continue,
            Phase::Gas => AssetProfile::GasCloud,
            Phase::Solid if material.representation == RepresentationKind::Metallic => {
                AssetProfile::MetalChunk
            }
            Phase::Solid => AssetProfile::PowderPile,
        };
        let x = if surface_oxidation {
            0
        } else if reactant_slot % 2 == 0 {
            -280
        } else {
            280
        };
        reactant_slot = reactant_slot.saturating_add(1);
        objects.push(PresentationObject {
            id: material.binding.clone(),
            asset,
            semantic_identity: material.semantic_identity.clone(),
            appearance: appearance_for_material(material),
            role: SceneRole::Reactant,
            transform: if surface_oxidation {
                transform([x, 0, 0], [1_350, 1_350, 1_350])
            } else {
                transform([x, 610, 0], [650, 650, 650])
            },
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
                Phase::Unknown
                    if surface_oxidation
                        && material.role == MacroscopicMaterialRole::Product
                        && material.representation == RepresentationKind::Ionic =>
                {
                    push_coloured_process_effect(
                        &mut effects,
                        EffectProfile::SurfaceOxidation,
                        MacroscopicProcess::SurfaceOxidation,
                        *ordinal,
                        final_ordinal,
                        reaction.intensity,
                        surface_oxide_colour.clone(),
                    );
                }
                // An unreviewed phase authorizes no formation effects.
                Phase::Unknown => {}
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
                Phase::Solid if surface_oxidation => {
                    push_coloured_process_effect(
                        &mut effects,
                        EffectProfile::SurfaceOxidation,
                        MacroscopicProcess::SurfaceOxidation,
                        *ordinal,
                        final_ordinal,
                        reaction.intensity,
                        surface_oxide_colour.clone(),
                    );
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
                    } else {
                        push_effect(
                            &mut effects,
                            EffectProfile::SolidFormation,
                            *predicate,
                            *ordinal,
                            final_ordinal,
                            reaction.intensity,
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
                if surface_oxidation
                    && matches!(material.phase, Phase::Solid | Phase::Unknown)
                    && material.representation == RepresentationKind::Metallic
                {
                    continue;
                }
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
    if reaction.process == Some(MacroscopicProcess::AqueousPrecipitation)
        && !effects
            .iter()
            .any(|effect| effect.effect == EffectProfile::PrecipitateFormation)
    {
        let formation_ordinal =
            first_product_assignment_ordinal(frames).unwrap_or_else(|| final_ordinal.min(1));
        if let Some(precipitate) = reaction.materials.iter().find(|material| {
            material.role == MacroscopicMaterialRole::Product && material.phase == Phase::Solid
        }) {
            add_process_precipitate_object(
                &mut objects,
                precipitate,
                formation_ordinal,
                &transform,
            );
            push_process_effect(
                &mut effects,
                EffectProfile::PrecipitateFormation,
                MacroscopicProcess::AqueousPrecipitation,
                formation_ordinal,
                final_ordinal,
                reaction.intensity,
            );
            push_process_effect(
                &mut effects,
                EffectProfile::Clouding,
                MacroscopicProcess::AqueousPrecipitation,
                formation_ordinal,
                final_ordinal,
                EffectIntensity::Subtle,
            );
        }
    }
    if matches!(
        reaction.process,
        Some(
            MacroscopicProcess::GasEvolutionLiquidLiquid
                | MacroscopicProcess::GasEvolutionSolidLiquid
        )
    ) && !effects
        .iter()
        .any(|effect| effect.effect == EffectProfile::GasRelease)
    {
        let generation_ordinal =
            first_product_assignment_ordinal(frames).unwrap_or_else(|| final_ordinal.min(1));
        let gas_products = reaction
            .materials
            .iter()
            .filter(|material| {
                material.role == MacroscopicMaterialRole::Product && material.phase == Phase::Gas
            })
            .collect::<Vec<_>>();
        if let [gas_product] = gas_products.as_slice() {
            let Some(
                process @ (MacroscopicProcess::GasEvolutionLiquidLiquid
                | MacroscopicProcess::GasEvolutionSolidLiquid),
            ) = reaction.process
            else {
                return Err(PhaseDrivenProfileError::InvalidGasEvolutionProcess);
            };
            add_process_gas_product_object(
                &mut objects,
                gas_product,
                generation_ordinal,
                &transform,
            );
            for effect in [
                EffectProfile::GasRelease,
                EffectProfile::BubbleEmitter,
                EffectProfile::SurfaceDisturbance,
            ] {
                push_gas_process_effect(
                    &mut effects,
                    effect,
                    process,
                    generation_ordinal,
                    final_ordinal,
                    reaction.intensity,
                );
            }
        }
    }
    if surface_oxidation
        && !effects
            .iter()
            .any(|effect| effect.effect == EffectProfile::SurfaceOxidation)
    {
        // Dynamic mechanisms do not always retain a renderer-bound `forms`
        // observation even though the validated macroscopic classifier has
        // already established solid metal + dioxygen -> solid ionic oxide.
        // The typed process is sufficient authority for the reusable surface
        // transition, just as it is for precipitation and gas evolution.
        let formation_ordinal =
            first_product_assignment_ordinal(frames).unwrap_or_else(|| final_ordinal.min(1));
        push_coloured_process_effect(
            &mut effects,
            EffectProfile::SurfaceOxidation,
            MacroscopicProcess::SurfaceOxidation,
            formation_ordinal,
            final_ordinal,
            reaction.intensity,
            surface_oxide_colour,
        );
    }
    if reaction.process == Some(MacroscopicProcess::MetalDisplacement)
        && !effects
            .iter()
            .any(|effect| effect.effect == EffectProfile::SolidFormation)
    {
        let formation_ordinal =
            first_product_assignment_ordinal(frames).unwrap_or_else(|| final_ordinal.min(1));
        for effect in [
            EffectProfile::SolidFormation,
            EffectProfile::LiquidMixing,
            EffectProfile::SurfaceDisturbance,
        ] {
            push_process_effect(
                &mut effects,
                effect,
                MacroscopicProcess::MetalDisplacement,
                formation_ordinal,
                final_ordinal,
                reaction.intensity,
            );
        }
    }
    if reaction.process == Some(MacroscopicProcess::SolidSolidSynthesis)
        && !effects
            .iter()
            .any(|effect| effect.effect == EffectProfile::SolidFormation)
    {
        let formation_ordinal =
            first_product_assignment_ordinal(frames).unwrap_or_else(|| final_ordinal.min(1));
        for effect in [
            EffectProfile::SolidFormation,
            EffectProfile::ReactionActivity,
        ] {
            push_process_effect(
                &mut effects,
                effect,
                MacroscopicProcess::SolidSolidSynthesis,
                formation_ordinal,
                final_ordinal,
                reaction.intensity,
            );
        }
    }
    if let Some(
        process @ (MacroscopicProcess::CompleteCombustion
        | MacroscopicProcess::IncompleteCombustion),
    ) = reaction.process
    {
        let start_ordinal =
            first_product_assignment_ordinal(frames).unwrap_or_else(|| final_ordinal.min(1));
        push_process_effect(
            &mut effects,
            EffectProfile::FlameEmitter(FlamePalette::Natural),
            process,
            start_ordinal,
            final_ordinal,
            EffectIntensity::Strong,
        );
        push_process_effect(
            &mut effects,
            EffectProfile::VapourRelease,
            process,
            start_ordinal,
            final_ordinal,
            reaction.intensity,
        );
        if has_mobile_reactant {
            push_process_effect(
                &mut effects,
                EffectProfile::SurfaceDisturbance,
                process,
                start_ordinal,
                final_ordinal,
                EffectIntensity::Moderate,
            );
        }
    }
    if let Some(process @ MacroscopicProcess::ExplosiveMetalWater(_)) = reaction.process {
        let start_ordinal =
            first_product_assignment_ordinal(frames).unwrap_or_else(|| final_ordinal.min(1));
        for effect in [
            EffectProfile::FlameEmitter(FlamePalette::Natural),
            EffectProfile::VapourRelease,
            EffectProfile::SplashEmitter,
            EffectProfile::HeatDistortion,
        ] {
            push_process_effect(
                &mut effects,
                effect,
                process,
                start_ordinal,
                final_ordinal,
                EffectIntensity::Strong,
            );
        }
    }

    let mut profile = PresentationProfile {
        id: reaction.profile_id.clone(),
        environment: AssetProfile::LaboratoryBench,
        objects,
        effects,
        camera: vec![CameraCue {
            behaviour: CameraBehaviour::WideEstablishingShot,
            start_ordinal: 0,
            end_ordinal: final_ordinal,
        }],
        precipitation: None,
        gas_evolution: None,
        metal_displacement: None,
        solid_solid_synthesis: None,
        explosive_metal_water: None,
        post_process: (reaction.process
            == Some(MacroscopicProcess::SolventEvaporationCrystallization))
        .then_some(MacroscopicProcess::SolventEvaporationCrystallization),
        equation: reaction.equation.clone(),
        disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    };
    authorize_explosive_metal_water_assembly(&mut profile, reaction, &active);
    authorize_precipitation_assembly(&mut profile, Some((reaction, &active)));
    authorize_gas_evolution_assembly(&mut profile, reaction, &active);
    authorize_metal_displacement_assembly(&mut profile, reaction, &active);
    authorize_solid_solid_synthesis_assembly(&mut profile, reaction, &active);
    Ok(profile)
}

/// Adds conservative reusable motion to an otherwise inert macroscopic
/// profile using only its already-authorized object roles/assets and the
/// validated `forms` observation.
///
/// This is primarily the backwards-compatible bridge for reviewed profiles
/// authored before macroscopic phase records existed. It never changes a
/// product asset or infers a phase from a reaction/species name.
///
/// # Errors
///
/// Returns an error when the validated frame range cannot be represented.
pub fn complete_generic_visual_profile(
    frames: &SimulationFrames,
    mut profile: PresentationProfile,
) -> Result<PresentationProfile, PhaseDrivenProfileError> {
    let final_ordinal = frames
        .frames()
        .last()
        .and_then(|frame| u16::try_from(frame.ordinal()).ok())
        .ok_or(PhaseDrivenProfileError::PresentationRange)?;
    let forms_ordinal = frames.frames().iter().find_map(|frame| {
        frame
            .observations()
            .iter()
            .any(|observation| {
                observation.status == ObservationStatus::Active
                    && observation.predicate == ObservationPredicate::Forms
            })
            .then(|| u16::try_from(frame.ordinal()).ok())
            .flatten()
    });
    let Some(forms_ordinal) = forms_ordinal else {
        authorize_gas_evolution_from_objects(&mut profile);
        return Ok(profile);
    };
    let was_inert = profile.effects.is_empty();
    if was_inert {
        push_effect(
            &mut profile.effects,
            EffectProfile::ReactionActivity,
            ObservationPredicate::Forms,
            forms_ordinal,
            final_ordinal,
            EffectIntensity::Subtle,
        );
    }

    let has_product = |assets: &[AssetProfile]| {
        profile
            .objects
            .iter()
            .any(|object| object.role == SceneRole::Product && assets.contains(&object.asset))
    };
    let has_gas_release = profile
        .effects
        .iter()
        .any(|effect| effect.effect == EffectProfile::GasRelease);
    if has_product(&[AssetProfile::GasCloud]) && !has_gas_release {
        push_effect(
            &mut profile.effects,
            EffectProfile::GasRelease,
            ObservationPredicate::Forms,
            forms_ordinal,
            final_ordinal,
            EffectIntensity::Moderate,
        );
    }
    if has_product(&[
        AssetProfile::MetalChunk,
        AssetProfile::MetalStrip,
        AssetProfile::CrystalCluster,
        AssetProfile::PowderPile,
    ]) && !profile.effects.iter().any(|effect| {
        matches!(
            effect.effect,
            EffectProfile::SolidFormation | EffectProfile::PrecipitateFormation
        )
    }) {
        push_effect(
            &mut profile.effects,
            EffectProfile::SolidFormation,
            ObservationPredicate::Forms,
            forms_ordinal,
            final_ordinal,
            EffectIntensity::Moderate,
        );
    }
    if was_inert
        && profile.objects.iter().any(|object| {
            matches!(object.role, SceneRole::Contents | SceneRole::Product)
                && object.asset == AssetProfile::LiquidVolume
        })
        && !profile
            .effects
            .iter()
            .any(|effect| effect.effect == EffectProfile::LiquidMixing)
    {
        push_effect(
            &mut profile.effects,
            EffectProfile::LiquidMixing,
            ObservationPredicate::Forms,
            forms_ordinal,
            final_ordinal,
            EffectIntensity::Subtle,
        );
    }
    if profile.precipitation.is_none() {
        authorize_precipitation_assembly(&mut profile, None);
    }
    authorize_gas_evolution_from_objects(&mut profile);
    Ok(profile)
}

const COLOURLESS_LIQUID: VisualColour = VisualColour {
    red: 0xd8,
    green: 0xe3,
    blue: 0xe8,
};
const OFF_WHITE_PRECIPITATE: VisualColour = VisualColour {
    red: 0xeb,
    green: 0xe9,
    blue: 0xda,
};
const PALE_COLOURLESS_GAS: VisualColour = VisualColour {
    red: 0xd8,
    green: 0xe3,
    blue: 0xe8,
};
const NEUTRAL_METAL: VisualColour = VisualColour {
    red: 0xc7,
    green: 0xcb,
    blue: 0xce,
};
const NEUTRAL_DEPOSITED_METAL: VisualColour = VisualColour {
    red: 0xb9,
    green: 0xbe,
    blue: 0xc1,
};

fn authorize_gas_evolution_from_objects(profile: &mut PresentationProfile) {
    if profile.gas_evolution.is_some()
        || profile.precipitation.is_some()
        || profile.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && matches!(
                    object.asset,
                    AssetProfile::CompleteCombustionAssembly
                        | AssetProfile::IncompleteCombustionAssembly
                )
        })
    {
        return;
    }
    let Some(effect) = profile.effects.iter().find(|effect| {
        effect.effect == EffectProfile::GasRelease
            && matches!(
                effect.authorization,
                EffectAuthorization::Observation(
                    ObservationPredicate::Evolves | ObservationPredicate::Forms
                )
            )
    }) else {
        return;
    };
    let Some(product) = profile.objects.iter().find(|object| {
        object.role == SceneRole::Product
            && object.asset == AssetProfile::GasCloud
            && object.visible_from_ordinal == effect.start_ordinal
            && object.observation.as_ref().is_some_and(|binding| {
                binding.predicate == effect.trigger && binding.value.is_none()
            })
    }) else {
        return;
    };
    let mobile = profile
        .objects
        .iter()
        .filter(|object| {
            matches!(object.role, SceneRole::Reactant | SceneRole::Contents)
                && object.asset == AssetProfile::LiquidVolume
        })
        .collect::<Vec<_>>();
    let solids = profile
        .objects
        .iter()
        .filter(|object| {
            object.role == SceneRole::Reactant
                && matches!(
                    object.asset,
                    AssetProfile::MetalChunk
                        | AssetProfile::MetalStrip
                        | AssetProfile::PowderPile
                        | AssetProfile::CrystalCluster
                )
        })
        .collect::<Vec<_>>();
    let (variant, initial, added) = match (mobile.as_slice(), solids.as_slice()) {
        ([first, second, ..], []) if first.id != second.id => {
            (GasEvolutionVariant::LiquidLiquid, *first, *second)
        }
        ([liquid, ..], [solid, ..]) if liquid.id != solid.id => {
            (GasEvolutionVariant::SolidLiquid, *liquid, *solid)
        }
        _ => return,
    };
    let colour_for = |object: &PresentationObject, fallback| {
        let base_colour = match object.appearance {
            AppearanceProfile::ReviewedColour(colour) => colour,
            _ => fallback,
        };
        BoundVisualColour {
            binding: object.id.clone(),
            base_colour,
            colour: object
                .colour_transition
                .as_ref()
                .map_or(base_colour, |transition| transition.target),
            transition_ordinal: object
                .colour_transition
                .as_ref()
                .map(|transition| transition.start_ordinal),
        }
    };
    profile.gas_evolution = Some(GasEvolutionVisualProfile {
        generation_ordinal: effect.start_ordinal,
        variant,
        initial_reactant: colour_for(initial, COLOURLESS_LIQUID),
        added_reactant: colour_for(
            added,
            if variant == GasEvolutionVariant::LiquidLiquid {
                COLOURLESS_LIQUID
            } else {
                OFF_WHITE_PRECIPITATE
            },
        ),
        gas_product: colour_for(product, PALE_COLOURLESS_GAS),
    });
}

fn authorize_explosive_metal_water_assembly(
    profile: &mut PresentationProfile,
    reaction: &MacroscopicReaction,
    active: &ActiveObservations,
) {
    let Some(MacroscopicProcess::ExplosiveMetalWater(variant)) = reaction.process else {
        return;
    };
    if profile.precipitation.is_some()
        || profile.gas_evolution.is_some()
        || profile.metal_displacement.is_some()
        || profile.solid_solid_synthesis.is_some()
    {
        return;
    }
    let Some((metal, water)) = explosive_metal_water_reactants(reaction, variant) else {
        return;
    };
    let Some((hydroxide, hydrogen)) = explosive_metal_water_products(reaction) else {
        return;
    };
    let contact_ordinal = profile.effects.iter().find_map(|effect| {
        (effect.effect == EffectProfile::HeatDistortion
            && effect.authorization
                == EffectAuthorization::Process(MacroscopicProcess::ExplosiveMetalWater(variant)))
        .then_some(effect.start_ordinal)
    });
    let Some(contact_ordinal) = contact_ordinal else {
        return;
    };
    let gas_object_is_bound = profile.objects.iter().any(|object| {
        object.role == SceneRole::Product
            && object.asset == AssetProfile::GasCloud
            && object.id == hydrogen.binding
            && object.visible_from_ordinal >= contact_ordinal
    });
    if !gas_object_is_bound {
        return;
    }
    let Some(vessel) = profile
        .objects
        .iter_mut()
        .find(|object| object.role == SceneRole::Vessel)
    else {
        return;
    };
    vessel.asset = AssetProfile::ExplosiveMetalWaterAssembly;
    profile.explosive_metal_water = Some(ExplosiveMetalWaterVisualProfile {
        contact_ordinal,
        variant,
        water_reactant: exact_bound_material_colour(water, COLOURLESS_LIQUID, active),
        metal_reactant: exact_bound_material_colour(metal, NEUTRAL_METAL, active),
        hydroxide_product: exact_bound_material_colour(hydroxide, COLOURLESS_LIQUID, active),
        hydrogen_product: exact_bound_material_colour(hydrogen, PALE_COLOURLESS_GAS, active),
    });
}

fn explosive_metal_water_reactants(
    reaction: &MacroscopicReaction,
    variant: ExplosiveMetalWaterVariant,
) -> Option<(&MacroscopicMaterial, &MacroscopicMaterial)> {
    let materials = reaction
        .materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Reactant)
        .collect::<Vec<_>>();
    let [first, second] = materials.as_slice() else {
        return None;
    };
    let valid_layout = |metal: &MacroscopicMaterial, water: &MacroscopicMaterial| {
        metal.phase == Phase::Solid
            && metal.representation == RepresentationKind::Metallic
            && metal.explosive_water_contact == Some(variant)
            && water.phase == Phase::Liquid
            && water.representation == RepresentationKind::Molecular
    };
    if valid_layout(first, second) {
        Some((first, second))
    } else {
        valid_layout(second, first).then_some((second, first))
    }
}

fn explosive_metal_water_products(
    reaction: &MacroscopicReaction,
) -> Option<(&MacroscopicMaterial, &MacroscopicMaterial)> {
    let materials = reaction
        .materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Product)
        .collect::<Vec<_>>();
    let [first, second] = materials.as_slice() else {
        return None;
    };
    let valid_layout = |hydroxide: &MacroscopicMaterial, hydrogen: &MacroscopicMaterial| {
        hydroxide.phase == Phase::Aqueous
            && hydroxide.representation == RepresentationKind::Ionic
            && hydrogen.phase == Phase::Gas
            && hydrogen.representation == RepresentationKind::Molecular
    };
    if valid_layout(first, second) {
        Some((first, second))
    } else {
        valid_layout(second, first).then_some((second, first))
    }
}

fn exact_bound_material_colour(
    material: &MacroscopicMaterial,
    fallback: VisualColour,
    active: &ActiveObservations,
) -> BoundVisualColour {
    let base_colour = material.colour.unwrap_or(fallback);
    let exact = active
        .get(&(material.binding.clone(), ObservationPredicate::Colour))
        .and_then(|(ordinal, value)| {
            value
                .as_deref()
                .and_then(visual_colour)
                .map(|colour| (*ordinal, colour))
        });
    BoundVisualColour {
        binding: material.binding.clone(),
        base_colour,
        colour: exact.map_or(base_colour, |(_, colour)| colour),
        transition_ordinal: exact.map(|(ordinal, _)| ordinal),
    }
}

fn authorize_precipitation_assembly(
    profile: &mut PresentationProfile,
    phase_data: Option<(&MacroscopicReaction, &ActiveObservations)>,
) {
    let Some(formation_ordinal) = authorized_precipitation_ordinal(profile) else {
        return;
    };
    let process_authorized = precipitation_process_authorized(profile, formation_ordinal);
    let Some(product) = profile.objects.iter().find(|object| {
        object.role == SceneRole::Product
            && object.asset == AssetProfile::PrecipitateCloud
            && object.visible_from_ordinal == formation_ordinal
            && (object
                .observation
                .as_ref()
                .is_some_and(|binding| binding.predicate == ObservationPredicate::Forms)
                || (process_authorized && object.observation.is_none()))
    }) else {
        return;
    };
    if !profile.objects.iter().any(|object| {
        object.role == SceneRole::Contents && object.asset == AssetProfile::LiquidVolume
    }) {
        return;
    }

    let colours = phase_data
        .and_then(|(reaction, active)| {
            precipitation_colours_from_materials(reaction, active, product, formation_ordinal)
        })
        .or_else(|| precipitation_colours_from_objects(profile, product, formation_ordinal));
    let Some(colours) = colours else {
        return;
    };
    let Some(vessel) = profile
        .objects
        .iter_mut()
        .find(|object| object.role == SceneRole::Vessel)
    else {
        return;
    };
    vessel.asset = AssetProfile::AqueousPrecipitationAssembly;
    profile.precipitation = Some(colours);
}

fn authorized_precipitation_ordinal(profile: &PresentationProfile) -> Option<u16> {
    let (ordinal, authorization) = profile.effects.iter().find_map(|effect| {
        (effect.effect == EffectProfile::PrecipitateFormation
            && effect.trigger == ObservationPredicate::Forms
            && matches!(
                effect.authorization,
                EffectAuthorization::Observation(ObservationPredicate::Forms)
                    | EffectAuthorization::Process(MacroscopicProcess::AqueousPrecipitation)
            ))
        .then_some((effect.start_ordinal, effect.authorization))
    })?;
    profile
        .effects
        .iter()
        .any(|effect| {
            effect.effect == EffectProfile::Clouding
                && effect.trigger == ObservationPredicate::Forms
                && effect.authorization == authorization
                && effect.start_ordinal == ordinal
        })
        .then_some(ordinal)
}

fn precipitation_process_authorized(profile: &PresentationProfile, ordinal: u16) -> bool {
    profile.effects.iter().any(|effect| {
        effect.effect == EffectProfile::PrecipitateFormation
            && effect.authorization
                == EffectAuthorization::Process(MacroscopicProcess::AqueousPrecipitation)
            && effect.start_ordinal == ordinal
    }) && profile.effects.iter().any(|effect| {
        effect.effect == EffectProfile::Clouding
            && effect.authorization
                == EffectAuthorization::Process(MacroscopicProcess::AqueousPrecipitation)
            && effect.start_ordinal == ordinal
    })
}

fn precipitation_colours_from_materials(
    reaction: &MacroscopicReaction,
    active: &ActiveObservations,
    product: &PresentationObject,
    formation_ordinal: u16,
) -> Option<PrecipitationVisualProfile> {
    let mobile = reaction
        .materials
        .iter()
        .filter(|material| {
            material.role == MacroscopicMaterialRole::Reactant
                && matches!(material.phase, Phase::Aqueous | Phase::Liquid)
        })
        .collect::<Vec<_>>();
    let [initial, added, ..] = mobile.as_slice() else {
        return None;
    };
    let precipitate = reaction.materials.iter().find(|material| {
        material.role == MacroscopicMaterialRole::Product
            && material.phase == Phase::Solid
            && material.binding == product.id
    })?;
    let colour_for = |material: &MacroscopicMaterial, fallback| {
        let base_colour = material.colour.unwrap_or(fallback);
        let exact = active
            .get(&(material.binding.clone(), ObservationPredicate::Colour))
            .and_then(|(ordinal, value)| {
                value
                    .as_deref()
                    .and_then(visual_colour)
                    .map(|colour| (*ordinal, colour))
            });
        BoundVisualColour {
            binding: material.binding.clone(),
            base_colour,
            colour: exact.map_or(base_colour, |(_, colour)| colour),
            transition_ordinal: exact.map(|(ordinal, _)| ordinal),
        }
    };
    Some(PrecipitationVisualProfile {
        formation_ordinal,
        initial_liquid: colour_for(initial, COLOURLESS_LIQUID),
        added_liquid: colour_for(added, COLOURLESS_LIQUID),
        precipitate: colour_for(precipitate, OFF_WHITE_PRECIPITATE),
    })
}

fn precipitation_colours_from_objects(
    profile: &PresentationProfile,
    product: &PresentationObject,
    formation_ordinal: u16,
) -> Option<PrecipitationVisualProfile> {
    let liquids = profile
        .objects
        .iter()
        .filter(|object| {
            object.role == SceneRole::Contents && object.asset == AssetProfile::LiquidVolume
        })
        .collect::<Vec<_>>();
    let initial = *liquids.first()?;
    let added = liquids.get(1).copied().unwrap_or(initial);
    let object_colour = |object: &PresentationObject, fallback| match object.appearance {
        AppearanceProfile::ReviewedColour(colour) => colour,
        AppearanceProfile::Water | AppearanceProfile::AqueousColourless => COLOURLESS_LIQUID,
        AppearanceProfile::WhitePrecipitate => VisualColour {
            red: 0xf0,
            green: 0xf5,
            blue: 0xfa,
        },
        AppearanceProfile::CreamPrecipitate => VisualColour {
            red: 0xf0,
            green: 0xe0,
            blue: 0xad,
        },
        AppearanceProfile::YellowPrecipitate => VisualColour {
            red: 0xef,
            green: 0xd1,
            blue: 0x47,
        },
        _ => fallback,
    };
    let product_binding = product.colour_transition.as_ref().map_or_else(
        || product.id.clone(),
        |transition| transition.subject_binding.clone(),
    );
    let product_colour = product.colour_transition.as_ref().map_or_else(
        || object_colour(product, OFF_WHITE_PRECIPITATE),
        |transition| transition.target,
    );
    Some(PrecipitationVisualProfile {
        formation_ordinal,
        initial_liquid: BoundVisualColour {
            binding: initial.id.clone(),
            base_colour: object_colour(initial, COLOURLESS_LIQUID),
            colour: object_colour(initial, COLOURLESS_LIQUID),
            transition_ordinal: None,
        },
        added_liquid: BoundVisualColour {
            binding: added.id.clone(),
            base_colour: object_colour(added, COLOURLESS_LIQUID),
            colour: object_colour(added, COLOURLESS_LIQUID),
            transition_ordinal: None,
        },
        precipitate: BoundVisualColour {
            binding: product_binding,
            base_colour: object_colour(product, OFF_WHITE_PRECIPITATE),
            colour: product_colour,
            transition_ordinal: product
                .colour_transition
                .as_ref()
                .map(|transition| transition.start_ordinal),
        },
    })
}

#[allow(clippy::too_many_lines)]
fn authorize_gas_evolution_assembly(
    profile: &mut PresentationProfile,
    reaction: &MacroscopicReaction,
    active: &ActiveObservations,
) {
    if matches!(
        reaction.process,
        Some(
            MacroscopicProcess::CompleteCombustion
                | MacroscopicProcess::IncompleteCombustion
                | MacroscopicProcess::ExplosiveMetalWater(_)
        )
    ) || profile.precipitation.is_some()
    {
        return;
    }

    let reactants = reaction
        .materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Reactant)
        .collect::<Vec<_>>();
    let [first, second] = reactants.as_slice() else {
        return;
    };
    let mobile =
        |material: &MacroscopicMaterial| matches!(material.phase, Phase::Aqueous | Phase::Liquid);
    let (variant, initial, added) = match (mobile(first), mobile(second), first.phase, second.phase)
    {
        (true, true, _, _) => (GasEvolutionVariant::LiquidLiquid, *first, *second),
        (true, false, _, Phase::Solid) => (GasEvolutionVariant::SolidLiquid, *first, *second),
        (false, true, Phase::Solid, _) => (GasEvolutionVariant::SolidLiquid, *second, *first),
        _ => return,
    };
    if matches!(
        (reaction.process, variant),
        (
            Some(MacroscopicProcess::GasEvolutionLiquidLiquid),
            GasEvolutionVariant::SolidLiquid
        ) | (
            Some(MacroscopicProcess::GasEvolutionSolidLiquid),
            GasEvolutionVariant::LiquidLiquid
        )
    ) {
        return;
    }
    let gas_products = reaction
        .materials
        .iter()
        .filter(|material| {
            material.role == MacroscopicMaterialRole::Product && material.phase == Phase::Gas
        })
        .collect::<Vec<_>>();
    let [gas_product] = gas_products.as_slice() else {
        return;
    };
    let generation = active
        .get(&(gas_product.binding.clone(), ObservationPredicate::Evolves))
        .map(|(ordinal, _)| {
            (
                *ordinal,
                ObservationPredicate::Evolves,
                EffectAuthorization::Observation(ObservationPredicate::Evolves),
            )
        })
        .or_else(|| {
            active
                .get(&(gas_product.binding.clone(), ObservationPredicate::Forms))
                .map(|(ordinal, _)| {
                    (
                        *ordinal,
                        ObservationPredicate::Forms,
                        EffectAuthorization::Observation(ObservationPredicate::Forms),
                    )
                })
        })
        .or_else(|| {
            reaction.process.and_then(|process| {
                matches!(
                    process,
                    MacroscopicProcess::GasEvolutionLiquidLiquid
                        | MacroscopicProcess::GasEvolutionSolidLiquid
                )
                .then(|| {
                    profile.effects.iter().find_map(|effect| {
                        (effect.effect == EffectProfile::GasRelease
                            && effect.trigger == ObservationPredicate::Evolves
                            && effect.authorization == EffectAuthorization::Process(process))
                        .then_some((
                            effect.start_ordinal,
                            ObservationPredicate::Evolves,
                            effect.authorization,
                        ))
                    })
                })
                .flatten()
            })
        });
    let Some((generation_ordinal, predicate, authorization)) = generation else {
        return;
    };
    let effect_authorized = profile.effects.iter().any(|effect| {
        effect.effect == EffectProfile::GasRelease
            && effect.trigger == predicate
            && effect.authorization == authorization
            && effect.start_ordinal == generation_ordinal
    });
    let product_authorized = profile.objects.iter().any(|object| {
        object.role == SceneRole::Product
            && object.asset == AssetProfile::GasCloud
            && object.id == gas_product.binding
            && object.visible_from_ordinal == generation_ordinal
            && (object.observation.as_ref().is_some_and(|observation| {
                observation.predicate == predicate && observation.value.is_none()
            }) || (matches!(
                authorization,
                EffectAuthorization::Process(
                    MacroscopicProcess::GasEvolutionLiquidLiquid
                        | MacroscopicProcess::GasEvolutionSolidLiquid
                )
            ) && object.observation.is_none()))
    });
    if !effect_authorized || !product_authorized {
        return;
    }

    let colour_for = |material: &MacroscopicMaterial, fallback| {
        let base_colour = material.colour.unwrap_or(fallback);
        let exact = active
            .get(&(material.binding.clone(), ObservationPredicate::Colour))
            .and_then(|(ordinal, value)| {
                value
                    .as_deref()
                    .and_then(visual_colour)
                    .map(|colour| (*ordinal, colour))
            });
        BoundVisualColour {
            binding: material.binding.clone(),
            base_colour,
            colour: exact.map_or(base_colour, |(_, colour)| colour),
            transition_ordinal: exact.map(|(ordinal, _)| ordinal),
        }
    };
    profile.gas_evolution = Some(GasEvolutionVisualProfile {
        generation_ordinal,
        variant,
        initial_reactant: colour_for(initial, COLOURLESS_LIQUID),
        added_reactant: colour_for(
            added,
            if mobile(added) {
                COLOURLESS_LIQUID
            } else {
                OFF_WHITE_PRECIPITATE
            },
        ),
        gas_product: colour_for(gas_product, PALE_COLOURLESS_GAS),
    });
}

#[allow(clippy::too_many_lines)]
fn authorize_metal_displacement_assembly(
    profile: &mut PresentationProfile,
    reaction: &MacroscopicReaction,
    active: &ActiveObservations,
) {
    if reaction.process != Some(MacroscopicProcess::MetalDisplacement)
        || profile.precipitation.is_some()
        || profile.gas_evolution.is_some()
        || reaction.materials.iter().any(|material| {
            material.role == MacroscopicMaterialRole::Product && material.phase == Phase::Gas
        })
        || profile.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && matches!(
                    object.asset,
                    AssetProfile::CompleteCombustionAssembly
                        | AssetProfile::IncompleteCombustionAssembly
                )
        })
    {
        return;
    }
    let reactants = reaction
        .materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Reactant)
        .collect::<Vec<_>>();
    let [first, second] = reactants.as_slice() else {
        return;
    };
    let (original_metal, initial_solution) = match (
        first.phase,
        first.representation,
        second.phase,
        second.representation,
    ) {
        (Phase::Solid, RepresentationKind::Metallic, Phase::Aqueous, RepresentationKind::Ionic) => {
            (*first, *second)
        }
        (Phase::Aqueous, RepresentationKind::Ionic, Phase::Solid, RepresentationKind::Metallic) => {
            (*second, *first)
        }
        _ => return,
    };
    let products = reaction
        .materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Product)
        .collect::<Vec<_>>();
    let [first, second] = products.as_slice() else {
        return;
    };
    let (final_solution, deposited_metal) = match (
        first.phase,
        first.representation,
        second.phase,
        second.representation,
    ) {
        (Phase::Aqueous, RepresentationKind::Ionic, Phase::Solid, RepresentationKind::Metallic) => {
            (*first, *second)
        }
        (Phase::Solid, RepresentationKind::Metallic, Phase::Aqueous, RepresentationKind::Ionic) => {
            (*second, *first)
        }
        _ => return,
    };
    let formation_observation = active
        .get(&(deposited_metal.binding.clone(), ObservationPredicate::Forms))
        .map(|(ordinal, _)| {
            (
                *ordinal,
                EffectAuthorization::Observation(ObservationPredicate::Forms),
            )
        });
    let formation_process = profile.effects.iter().find_map(|effect| {
        (effect.effect == EffectProfile::SolidFormation
            && effect.trigger == ObservationPredicate::Forms
            && effect.authorization
                == EffectAuthorization::Process(MacroscopicProcess::MetalDisplacement))
        .then_some((effect.start_ordinal, effect.authorization))
    });
    let Some((formation_ordinal, authorization)) = formation_observation.or(formation_process)
    else {
        return;
    };
    if !profile.effects.iter().any(|effect| {
        effect.effect == EffectProfile::SolidFormation
            && effect.start_ordinal == formation_ordinal
            && effect.authorization == authorization
    }) {
        return;
    }
    let Some(vessel) = profile
        .objects
        .iter_mut()
        .find(|object| object.role == SceneRole::Vessel)
    else {
        return;
    };
    vessel.asset = AssetProfile::MetalDisplacementAssembly;

    let colour_for = |material: &MacroscopicMaterial, fallback| {
        let base_colour = material.colour.unwrap_or(fallback);
        let exact = active
            .get(&(material.binding.clone(), ObservationPredicate::Colour))
            .and_then(|(ordinal, value)| {
                value
                    .as_deref()
                    .and_then(visual_colour)
                    .map(|colour| (*ordinal, colour))
            });
        BoundVisualColour {
            binding: material.binding.clone(),
            base_colour,
            colour: exact.map_or(base_colour, |(_, colour)| colour),
            transition_ordinal: exact.map(|(ordinal, _)| ordinal),
        }
    };
    profile.metal_displacement = Some(MetalDisplacementVisualProfile {
        formation_ordinal,
        initial_solution: colour_for(initial_solution, COLOURLESS_LIQUID),
        final_solution: colour_for(final_solution, COLOURLESS_LIQUID),
        original_metal: colour_for(original_metal, NEUTRAL_METAL),
        deposited_metal: colour_for(deposited_metal, NEUTRAL_DEPOSITED_METAL),
    });
}

#[allow(clippy::too_many_lines)]
fn authorize_solid_solid_synthesis_assembly(
    profile: &mut PresentationProfile,
    reaction: &MacroscopicReaction,
    active: &ActiveObservations,
) {
    if reaction.process != Some(MacroscopicProcess::SolidSolidSynthesis)
        || profile.precipitation.is_some()
        || profile.gas_evolution.is_some()
        || profile.metal_displacement.is_some()
        || reaction.materials.iter().any(|material| {
            material.role == MacroscopicMaterialRole::Product && material.phase == Phase::Gas
        })
        || profile.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && matches!(
                    object.asset,
                    AssetProfile::CompleteCombustionAssembly
                        | AssetProfile::IncompleteCombustionAssembly
                )
        })
    {
        return;
    }
    let reactants = reaction
        .materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Reactant)
        .collect::<Vec<_>>();
    let [reactant_a, reactant_b] = reactants.as_slice() else {
        return;
    };
    if reactant_a.phase != Phase::Solid || reactant_b.phase != Phase::Solid {
        return;
    }
    let products = reaction
        .materials
        .iter()
        .filter(|material| material.role == MacroscopicMaterialRole::Product)
        .collect::<Vec<_>>();
    let [product] = products.as_slice() else {
        return;
    };
    if product.phase != Phase::Solid {
        return;
    }

    let formation_observation = active
        .get(&(product.binding.clone(), ObservationPredicate::Forms))
        .map(|(ordinal, _)| {
            (
                *ordinal,
                EffectAuthorization::Observation(ObservationPredicate::Forms),
            )
        });
    let formation_process = profile.effects.iter().find_map(|effect| {
        (effect.effect == EffectProfile::SolidFormation
            && effect.trigger == ObservationPredicate::Forms
            && effect.authorization
                == EffectAuthorization::Process(MacroscopicProcess::SolidSolidSynthesis))
        .then_some((effect.start_ordinal, effect.authorization))
    });
    let Some((formation_ordinal, authorization)) = formation_observation.or(formation_process)
    else {
        return;
    };
    if !profile.effects.iter().any(|effect| {
        effect.effect == EffectProfile::SolidFormation
            && effect.start_ordinal == formation_ordinal
            && effect.authorization == authorization
    }) {
        return;
    }
    let Some(vessel) = profile
        .objects
        .iter_mut()
        .find(|object| object.role == SceneRole::Vessel)
    else {
        return;
    };
    vessel.asset = AssetProfile::SolidSolidSynthesisAssembly;

    let colour_for = |material: &MacroscopicMaterial, fallback| {
        let base_colour = material.colour.unwrap_or(fallback);
        let exact = active
            .get(&(material.binding.clone(), ObservationPredicate::Colour))
            .and_then(|(ordinal, value)| {
                value
                    .as_deref()
                    .and_then(visual_colour)
                    .map(|colour| (*ordinal, colour))
            });
        BoundVisualColour {
            binding: material.binding.clone(),
            base_colour,
            colour: exact.map_or(base_colour, |(_, colour)| colour),
            transition_ordinal: exact.map(|(ordinal, _)| ordinal),
        }
    };
    profile.solid_solid_synthesis = Some(SolidSolidSynthesisVisualProfile {
        formation_ordinal,
        reactant_a: colour_for(reactant_a, NEUTRAL_METAL),
        reactant_b: colour_for(reactant_b, OFF_WHITE_PRECIPITATE),
        product: colour_for(product, NEUTRAL_DEPOSITED_METAL),
        show_reaction_front: profile.effects.iter().any(|effect| {
            effect.effect == EffectProfile::ReactionActivity
                && effect.authorization
                    == EffectAuthorization::Process(MacroscopicProcess::SolidSolidSynthesis)
        }),
    });
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

fn first_product_assignment_ordinal(frames: &SimulationFrames) -> Option<u16> {
    frames.frames().iter().find_map(|frame| {
        matches!(
            frame
                .active_operation()
                .map(|active| active.operation.view()),
            Some(StructuralOperationView::AssignProduct { .. })
        )
        .then(|| u16::try_from(frame.ordinal()).ok())
        .flatten()
    })
}

fn appearance_for_material(material: &MacroscopicMaterial) -> AppearanceProfile {
    if let Some(colour) = material.colour {
        return AppearanceProfile::ReviewedColour(colour);
    }
    match (material.phase, material.representation) {
        (Phase::Aqueous | Phase::Liquid | Phase::Gas, _) => AppearanceProfile::AqueousColourless,
        (Phase::Solid, RepresentationKind::Metallic) => AppearanceProfile::MetalSilver,
        (Phase::Solid | Phase::Unknown, _) => AppearanceProfile::LaboratoryNeutral,
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
        // An unreviewed phase never places product geometry.
        Phase::Unknown => return,
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

fn add_process_precipitate_object<F>(
    objects: &mut Vec<PresentationObject>,
    material: &MacroscopicMaterial,
    ordinal: u16,
    transform: &F,
) where
    F: Fn([i16; 3], [u16; 3]) -> PresentationTransform,
{
    if objects.iter().any(|object| object.id == material.binding) {
        return;
    }
    objects.push(PresentationObject {
        id: material.binding.clone(),
        asset: AssetProfile::PrecipitateCloud,
        semantic_identity: material.semantic_identity.clone(),
        appearance: appearance_for_material(material),
        role: SceneRole::Product,
        transform: transform([0, -520, 0], [760, 360, 760]),
        visible_from_ordinal: ordinal,
        observation: None,
        colour_transition: None,
    });
}

fn add_process_gas_product_object<F>(
    objects: &mut Vec<PresentationObject>,
    material: &MacroscopicMaterial,
    ordinal: u16,
    transform: &F,
) where
    F: Fn([i16; 3], [u16; 3]) -> PresentationTransform,
{
    if objects.iter().any(|object| object.id == material.binding) {
        return;
    }
    objects.push(PresentationObject {
        id: material.binding.clone(),
        asset: AssetProfile::GasCloud,
        semantic_identity: material.semantic_identity.clone(),
        appearance: appearance_for_material(material),
        role: SceneRole::Product,
        transform: transform([160, 930, 0], [620, 620, 620]),
        visible_from_ordinal: ordinal,
        observation: None,
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
        authorization: EffectAuthorization::Observation(trigger),
        intensity,
        start_ordinal,
        end_ordinal,
        surface_oxide_colour: None,
    };
    if !effects.contains(&candidate) {
        effects.push(candidate);
    }
}

fn push_process_effect(
    effects: &mut Vec<PresentationEffect>,
    effect: EffectProfile,
    process: MacroscopicProcess,
    start_ordinal: u16,
    end_ordinal: u16,
    intensity: EffectIntensity,
) {
    let candidate = PresentationEffect {
        effect,
        trigger: ObservationPredicate::Forms,
        authorization: EffectAuthorization::Process(process),
        intensity,
        start_ordinal,
        end_ordinal,
        surface_oxide_colour: None,
    };
    if !effects.contains(&candidate) {
        effects.push(candidate);
    }
}

fn push_gas_process_effect(
    effects: &mut Vec<PresentationEffect>,
    effect: EffectProfile,
    process: MacroscopicProcess,
    start_ordinal: u16,
    end_ordinal: u16,
    intensity: EffectIntensity,
) {
    let candidate = PresentationEffect {
        effect,
        trigger: ObservationPredicate::Evolves,
        authorization: EffectAuthorization::Process(process),
        intensity,
        start_ordinal,
        end_ordinal,
        surface_oxide_colour: None,
    };
    if !effects.contains(&candidate) {
        effects.push(candidate);
    }
}

fn push_coloured_process_effect(
    effects: &mut Vec<PresentationEffect>,
    effect: EffectProfile,
    process: MacroscopicProcess,
    start_ordinal: u16,
    end_ordinal: u16,
    intensity: EffectIntensity,
    surface_oxide_colour: Option<SurfaceOxideColour>,
) {
    let candidate = PresentationEffect {
        effect,
        trigger: ObservationPredicate::Forms,
        authorization: EffectAuthorization::Process(process),
        intensity,
        start_ordinal,
        end_ordinal,
        surface_oxide_colour,
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
    InvalidGasEvolutionProcess,
    InvalidSurfaceOxideColourBinding,
}

impl fmt::Display for PhaseDrivenProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PresentationRange => {
                formatter.write_str("validated frames exceed the presentation range")
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
            Self::InvalidGasEvolutionProcess => formatter
                .write_str("gas-evolution effects lack a compatible chemistry-owned process"),
            Self::InvalidSurfaceOxideColourBinding => formatter.write_str(
                "surface oxide colour is not bound to a validated product of this reaction",
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
    pub stage: MacroscopicStage,
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
    pub stage: MacroscopicStage,
}

/// Presentation-only stages layered after the immutable validated reaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroscopicStage {
    Reaction,
    HeatingPreparation,
    SolventBoiling,
    CrystalGrowth,
}

impl RealWorldTimeline {
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.beats.iter().fold(0_u64, |duration, beat| {
            duration.saturating_add(u64::from(beat.duration_ms))
        })
    }

    /// Returns one continuous wall-clock position for a located timeline
    /// sample. This is presentation time, not chemical extent: authored clips
    /// use it to avoid changing playback speed when adjacent chemistry beats
    /// have different durations.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn normalized_progress_at(&self, position: RealWorldPosition) -> f32 {
        let duration_ms = self.duration_ms();
        if duration_ms == 0 {
            return 1.0;
        }
        let elapsed_before = self
            .beats
            .iter()
            .take(position.beat_index)
            .fold(0_u64, |elapsed, beat| {
                elapsed.saturating_add(u64::from(beat.duration_ms))
            });
        let current_duration = self
            .beats
            .get(position.beat_index)
            .map_or(0, |beat| beat.duration_ms);
        let elapsed = elapsed_before as f32
            + current_duration as f32 * position.beat_progress.clamp(0.0, 1.0);
        (elapsed / duration_ms as f32).clamp(0.0, 1.0)
    }

    /// Returns deterministic absolute presentation milliseconds for a located
    /// position. Authored clips use this instead of accumulating frame deltas,
    /// so pause, seek, replay, and reverse scrubbing sample identically.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn elapsed_ms_at(&self, position: RealWorldPosition) -> Option<f32> {
        let current = self.beats.get(position.beat_index)?;
        let elapsed_before = self
            .beats
            .iter()
            .take(position.beat_index)
            .fold(0_u64, |elapsed, beat| {
                elapsed.saturating_add(u64::from(beat.duration_ms))
            });
        Some(
            elapsed_before as f32
                + current.duration_ms as f32 * position.beat_progress.clamp(0.0, 1.0),
        )
    }

    /// Absolute start time of the beat beginning at an observation ordinal.
    /// Effect activation ordinals are beat boundaries by construction.
    #[must_use]
    pub fn start_ms_for_ordinal(&self, ordinal: u16) -> Option<u64> {
        let mut elapsed = 0_u64;
        for beat in &self.beats {
            if beat.start_ordinal == ordinal {
                return Some(elapsed);
            }
            elapsed = elapsed.saturating_add(u64::from(beat.duration_ms));
        }
        None
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
                stage: beat.stage,
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
                stage: beat.stage,
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
    pub precipitation: Option<PrecipitationVisualProfile>,
    pub gas_evolution: Option<GasEvolutionVisualProfile>,
    pub metal_displacement: Option<MetalDisplacementVisualProfile>,
    pub solid_solid_synthesis: Option<SolidSolidSynthesisVisualProfile>,
    pub explosive_metal_water: Option<ExplosiveMetalWaterVisualProfile>,
    pub equation: String,
    pub annotations: Vec<MacroscopicAnnotation>,
    pub timeline: RealWorldTimeline,
    pub post_process: Option<MacroscopicProcess>,
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
/// observation, or the validated frame digest is unavailable.
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
    validate_real_world_profile(profile, &active_observations)?;
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
        precipitation: profile.precipitation.clone(),
        gas_evolution: profile.gas_evolution.clone(),
        metal_displacement: profile.metal_displacement.clone(),
        solid_solid_synthesis: profile.solid_solid_synthesis.clone(),
        explosive_metal_water: profile.explosive_metal_water.clone(),
        equation: profile.equation.clone(),
        annotations,
        timeline,
        post_process: profile.post_process,
        disclosure: profile.disclosure.clone(),
        virtual_only_disclosure: VIRTUAL_ONLY_DISCLOSURE.to_owned(),
    })
}

fn validate_real_world_profile(
    profile: &PresentationProfile,
    active_observations: &[(u16, &FrameObservation)],
) -> Result<(), PlanError> {
    if profile.effects.iter().any(|effect| {
        !effect_authorization_is_compatible(effect.effect, effect.trigger, effect.authorization)
            || (effect.surface_oxide_colour.is_some()
                && !matches!(
                    (effect.effect, effect.authorization),
                    (
                        EffectProfile::SurfaceOxidation,
                        EffectAuthorization::Process(MacroscopicProcess::SurfaceOxidation)
                    )
                ))
    }) {
        return Err(PlanError::IncompatibleEffectObservation);
    }
    if profile.effects.iter().any(|effect| {
        matches!(effect.authorization, EffectAuthorization::Observation(_))
            && active_observations
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
    validate_colour_transitions(profile, active_observations)?;
    validate_precipitation_profile(profile, active_observations)?;
    validate_gas_evolution_profile(profile, active_observations)?;
    validate_metal_displacement_profile(profile, active_observations)?;
    validate_solid_solid_synthesis_profile(profile, active_observations)?;
    validate_explosive_metal_water_profile(profile, active_observations)?;
    if profile.objects.iter().any(|object| {
        (object.role == SceneRole::Product
            && object.observation.is_none()
            && !product_has_process_authorization(profile, object))
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
    Ok(())
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

fn validate_precipitation_profile(
    profile: &PresentationProfile,
    active_observations: &[(u16, &FrameObservation)],
) -> Result<(), PlanError> {
    let assembly_selected = profile.objects.iter().any(|object| {
        object.role == SceneRole::Vessel
            && object.asset == AssetProfile::AqueousPrecipitationAssembly
    });
    let Some(precipitation) = &profile.precipitation else {
        return if assembly_selected {
            Err(PlanError::InvalidPrecipitationProfile)
        } else {
            Ok(())
        };
    };
    let ordinal = authorized_precipitation_ordinal(profile);
    let exact_product_forms = active_observations.iter().any(|(ordinal, observation)| {
        *ordinal == precipitation.formation_ordinal
            && observation.predicate == ObservationPredicate::Forms
            && observation.subject_binding == precipitation.precipitate.binding
    });
    let process_authorized =
        precipitation_process_authorized(profile, precipitation.formation_ordinal);
    let product_object = profile.objects.iter().any(|object| {
        object.role == SceneRole::Product
            && object.asset == AssetProfile::PrecipitateCloud
            && object.visible_from_ordinal == precipitation.formation_ordinal
            && (object.id == precipitation.precipitate.binding
                || object.colour_transition.as_ref().is_some_and(|transition| {
                    transition.subject_binding == precipitation.precipitate.binding
                }))
            && (object
                .observation
                .as_ref()
                .is_some_and(|binding| binding.predicate == ObservationPredicate::Forms)
                || (process_authorized && object.observation.is_none()))
    });
    let exact_colours_match = [
        &precipitation.initial_liquid,
        &precipitation.added_liquid,
        &precipitation.precipitate,
    ]
    .into_iter()
    .all(|bound| {
        active_observations
            .iter()
            .find(|(_, observation)| {
                observation.predicate == ObservationPredicate::Colour
                    && observation.subject_binding == bound.binding
            })
            .is_none_or(|(ordinal, observation)| {
                observation.value.as_deref().and_then(visual_colour) == Some(bound.colour)
                    && bound.transition_ordinal == Some(*ordinal)
            })
    });
    if !assembly_selected
        || ordinal != Some(precipitation.formation_ordinal)
        || !(exact_product_forms || process_authorized)
        || !product_object
        || !exact_colours_match
    {
        return Err(PlanError::InvalidPrecipitationProfile);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_gas_evolution_profile(
    profile: &PresentationProfile,
    active_observations: &[(u16, &FrameObservation)],
) -> Result<(), PlanError> {
    let Some(gas_evolution) = &profile.gas_evolution else {
        return Ok(());
    };
    if profile.objects.iter().any(|object| {
        object.role == SceneRole::Vessel
            && matches!(
                object.asset,
                AssetProfile::CompleteCombustionAssembly
                    | AssetProfile::IncompleteCombustionAssembly
            )
    }) {
        return Err(PlanError::InvalidGasEvolutionProfile);
    }
    let generation_observation = active_observations.iter().find(|(ordinal, observation)| {
        *ordinal == gas_evolution.generation_ordinal
            && observation.subject_binding == gas_evolution.gas_product.binding
            && matches!(
                observation.predicate,
                ObservationPredicate::Evolves | ObservationPredicate::Forms
            )
    });
    let process_authorization = profile.effects.iter().find_map(|effect| {
        (effect.effect == EffectProfile::GasRelease
            && effect.trigger == ObservationPredicate::Evolves
            && effect.start_ordinal == gas_evolution.generation_ordinal
            && matches!(
                effect.authorization,
                EffectAuthorization::Process(
                    MacroscopicProcess::GasEvolutionLiquidLiquid
                        | MacroscopicProcess::GasEvolutionSolidLiquid
                )
            ))
        .then_some(effect.authorization)
    });
    if process_authorization.is_some_and(|authorization| {
        !matches!(
            (authorization, gas_evolution.variant),
            (
                EffectAuthorization::Process(MacroscopicProcess::GasEvolutionLiquidLiquid),
                GasEvolutionVariant::LiquidLiquid
            ) | (
                EffectAuthorization::Process(MacroscopicProcess::GasEvolutionSolidLiquid),
                GasEvolutionVariant::SolidLiquid
            )
        )
    }) {
        return Err(PlanError::InvalidGasEvolutionProfile);
    }
    if generation_observation.is_none() && process_authorization.is_none() {
        return Err(PlanError::InvalidGasEvolutionProfile);
    }
    let product_object = profile.objects.iter().any(|object| {
        object.role == SceneRole::Product
            && object.asset == AssetProfile::GasCloud
            && object.id == gas_evolution.gas_product.binding
            && object.visible_from_ordinal == gas_evolution.generation_ordinal
            && (generation_observation.is_some_and(|(_, generation_observation)| {
                object
                    .observation
                    .as_ref()
                    .is_some_and(|binding| binding.predicate == generation_observation.predicate)
            }) || (process_authorization.is_some() && object.observation.is_none()))
    });
    let gas_effect = profile.effects.iter().any(|effect| {
        if let Some((_, generation_observation)) = generation_observation {
            effect.effect == EffectProfile::GasRelease
                && effect.trigger == generation_observation.predicate
                && effect.authorization
                    == EffectAuthorization::Observation(generation_observation.predicate)
                && effect.start_ordinal == gas_evolution.generation_ordinal
        } else {
            effect.effect == EffectProfile::GasRelease
                && effect.trigger == ObservationPredicate::Evolves
                && Some(effect.authorization) == process_authorization
                && effect.start_ordinal == gas_evolution.generation_ordinal
        }
    });
    let bindings_are_distinct = gas_evolution.initial_reactant.binding
        != gas_evolution.added_reactant.binding
        && gas_evolution.initial_reactant.binding != gas_evolution.gas_product.binding
        && gas_evolution.added_reactant.binding != gas_evolution.gas_product.binding;
    let exact_colours_match = [
        &gas_evolution.initial_reactant,
        &gas_evolution.added_reactant,
        &gas_evolution.gas_product,
    ]
    .into_iter()
    .all(|bound| {
        active_observations
            .iter()
            .find(|(_, observation)| {
                observation.predicate == ObservationPredicate::Colour
                    && observation.subject_binding == bound.binding
            })
            .is_none_or(|(ordinal, observation)| {
                observation.value.as_deref().and_then(visual_colour) == Some(bound.colour)
                    && bound.transition_ordinal == Some(*ordinal)
            })
    });
    if !product_object || !gas_effect || !bindings_are_distinct || !exact_colours_match {
        return Err(PlanError::InvalidGasEvolutionProfile);
    }
    Ok(())
}

fn validate_metal_displacement_profile(
    profile: &PresentationProfile,
    active_observations: &[(u16, &FrameObservation)],
) -> Result<(), PlanError> {
    let assembly_selected = profile.objects.iter().any(|object| {
        object.role == SceneRole::Vessel && object.asset == AssetProfile::MetalDisplacementAssembly
    });
    let Some(displacement) = &profile.metal_displacement else {
        return if assembly_selected {
            Err(PlanError::InvalidMetalDisplacementProfile)
        } else {
            Ok(())
        };
    };
    if !assembly_selected
        || profile.precipitation.is_some()
        || profile.gas_evolution.is_some()
        || profile.objects.iter().any(|object| {
            object.role == SceneRole::Vessel
                && matches!(
                    object.asset,
                    AssetProfile::CompleteCombustionAssembly
                        | AssetProfile::IncompleteCombustionAssembly
                )
        })
    {
        return Err(PlanError::InvalidMetalDisplacementProfile);
    }

    let exact_product_forms = active_observations.iter().any(|(ordinal, observation)| {
        *ordinal == displacement.formation_ordinal
            && observation.predicate == ObservationPredicate::Forms
            && observation.subject_binding == displacement.deposited_metal.binding
    });
    let process_authorized = profile.effects.iter().any(|effect| {
        effect.effect == EffectProfile::SolidFormation
            && effect.trigger == ObservationPredicate::Forms
            && effect.start_ordinal == displacement.formation_ordinal
            && effect.authorization
                == EffectAuthorization::Process(MacroscopicProcess::MetalDisplacement)
    });
    let bindings = [
        displacement.initial_solution.binding.as_str(),
        displacement.final_solution.binding.as_str(),
        displacement.original_metal.binding.as_str(),
        displacement.deposited_metal.binding.as_str(),
    ];
    let bindings_are_distinct = bindings
        .iter()
        .enumerate()
        .all(|(index, binding)| !bindings[..index].contains(binding));
    let exact_colours_match = [
        &displacement.initial_solution,
        &displacement.final_solution,
        &displacement.original_metal,
        &displacement.deposited_metal,
    ]
    .into_iter()
    .all(|bound| {
        active_observations
            .iter()
            .find(|(_, observation)| {
                observation.predicate == ObservationPredicate::Colour
                    && observation.subject_binding == bound.binding
            })
            .is_none_or(|(ordinal, observation)| {
                observation.value.as_deref().and_then(visual_colour) == Some(bound.colour)
                    && bound.transition_ordinal == Some(*ordinal)
            })
    });
    if !(exact_product_forms || process_authorized)
        || !bindings_are_distinct
        || !exact_colours_match
    {
        return Err(PlanError::InvalidMetalDisplacementProfile);
    }
    Ok(())
}

fn validate_solid_solid_synthesis_profile(
    profile: &PresentationProfile,
    active_observations: &[(u16, &FrameObservation)],
) -> Result<(), PlanError> {
    let assembly_selected = profile.objects.iter().any(|object| {
        object.role == SceneRole::Vessel
            && object.asset == AssetProfile::SolidSolidSynthesisAssembly
    });
    let Some(synthesis) = &profile.solid_solid_synthesis else {
        return if assembly_selected {
            Err(PlanError::InvalidSolidSolidSynthesisProfile)
        } else {
            Ok(())
        };
    };
    if !assembly_selected
        || profile.precipitation.is_some()
        || profile.gas_evolution.is_some()
        || profile.metal_displacement.is_some()
    {
        return Err(PlanError::InvalidSolidSolidSynthesisProfile);
    }
    let exact_product_forms = active_observations.iter().any(|(ordinal, observation)| {
        *ordinal == synthesis.formation_ordinal
            && observation.predicate == ObservationPredicate::Forms
            && observation.subject_binding == synthesis.product.binding
    });
    let process_authorized = profile.effects.iter().any(|effect| {
        effect.effect == EffectProfile::SolidFormation
            && effect.trigger == ObservationPredicate::Forms
            && effect.start_ordinal == synthesis.formation_ordinal
            && effect.authorization
                == EffectAuthorization::Process(MacroscopicProcess::SolidSolidSynthesis)
    });
    let bindings = [
        synthesis.reactant_a.binding.as_str(),
        synthesis.reactant_b.binding.as_str(),
        synthesis.product.binding.as_str(),
    ];
    let bindings_are_distinct = bindings
        .iter()
        .enumerate()
        .all(|(index, binding)| !bindings[..index].contains(binding));
    let exact_colours_match = [
        &synthesis.reactant_a,
        &synthesis.reactant_b,
        &synthesis.product,
    ]
    .into_iter()
    .all(|bound| {
        active_observations
            .iter()
            .find(|(_, observation)| {
                observation.predicate == ObservationPredicate::Colour
                    && observation.subject_binding == bound.binding
            })
            .is_none_or(|(ordinal, observation)| {
                observation.value.as_deref().and_then(visual_colour) == Some(bound.colour)
                    && bound.transition_ordinal == Some(*ordinal)
            })
    });
    if !(exact_product_forms || process_authorized)
        || !bindings_are_distinct
        || !exact_colours_match
    {
        return Err(PlanError::InvalidSolidSolidSynthesisProfile);
    }
    Ok(())
}

fn validate_explosive_metal_water_profile(
    profile: &PresentationProfile,
    active_observations: &[(u16, &FrameObservation)],
) -> Result<(), PlanError> {
    let assembly_selected = profile.objects.iter().any(|object| {
        object.role == SceneRole::Vessel
            && object.asset == AssetProfile::ExplosiveMetalWaterAssembly
    });
    let Some(explosive) = &profile.explosive_metal_water else {
        return if assembly_selected {
            Err(PlanError::InvalidExplosiveMetalWaterProfile)
        } else {
            Ok(())
        };
    };
    let bindings = [
        explosive.water_reactant.binding.as_str(),
        explosive.metal_reactant.binding.as_str(),
        explosive.hydroxide_product.binding.as_str(),
        explosive.hydrogen_product.binding.as_str(),
    ];
    let bindings_are_distinct = bindings
        .iter()
        .enumerate()
        .all(|(index, binding)| !bindings[..index].contains(binding));
    let process = MacroscopicProcess::ExplosiveMetalWater(explosive.variant);
    let effects_are_authorized = [
        EffectProfile::FlameEmitter(FlamePalette::Natural),
        EffectProfile::VapourRelease,
        EffectProfile::SplashEmitter,
        EffectProfile::HeatDistortion,
    ]
    .into_iter()
    .all(|expected| {
        profile.effects.iter().any(|effect| {
            effect.effect == expected
                && effect.start_ordinal == explosive.contact_ordinal
                && effect.authorization == EffectAuthorization::Process(process)
        })
    });
    let gas_object_is_bound = profile.objects.iter().any(|object| {
        object.role == SceneRole::Product
            && object.asset == AssetProfile::GasCloud
            && object.id == explosive.hydrogen_product.binding
            && object.visible_from_ordinal >= explosive.contact_ordinal
    });
    let exact_colours_match = [
        &explosive.water_reactant,
        &explosive.metal_reactant,
        &explosive.hydroxide_product,
        &explosive.hydrogen_product,
    ]
    .into_iter()
    .all(|bound| {
        active_observations
            .iter()
            .find(|(_, observation)| {
                observation.predicate == ObservationPredicate::Colour
                    && observation.subject_binding == bound.binding
            })
            .is_none_or(|(ordinal, observation)| {
                observation.value.as_deref().and_then(visual_colour) == Some(bound.colour)
                    && bound.transition_ordinal == Some(*ordinal)
            })
    });
    if !assembly_selected
        || !bindings_are_distinct
        || !effects_are_authorized
        || !gas_object_is_bound
        || !exact_colours_match
    {
        return Err(PlanError::InvalidExplosiveMetalWaterProfile);
    }
    Ok(())
}

fn effect_authorization_is_compatible(
    effect: EffectProfile,
    predicate: ObservationPredicate,
    authorization: EffectAuthorization,
) -> bool {
    if let EffectAuthorization::Process(process) = authorization {
        return matches!(
            (process, effect),
            (
                MacroscopicProcess::CompleteCombustion | MacroscopicProcess::IncompleteCombustion,
                EffectProfile::FlameEmitter(FlamePalette::Natural)
                    | EffectProfile::VapourRelease
                    | EffectProfile::SurfaceDisturbance
            ) | (
                MacroscopicProcess::AqueousPrecipitation,
                EffectProfile::PrecipitateFormation | EffectProfile::Clouding
            ) | (
                MacroscopicProcess::GasEvolutionLiquidLiquid
                    | MacroscopicProcess::GasEvolutionSolidLiquid,
                EffectProfile::GasRelease
                    | EffectProfile::BubbleEmitter
                    | EffectProfile::SurfaceDisturbance
            ) | (
                MacroscopicProcess::SurfaceOxidation,
                EffectProfile::SurfaceOxidation
            ) | (
                MacroscopicProcess::MetalDisplacement,
                EffectProfile::SolidFormation
                    | EffectProfile::LiquidMixing
                    | EffectProfile::SurfaceDisturbance
            ) | (
                MacroscopicProcess::SolidSolidSynthesis,
                EffectProfile::SolidFormation | EffectProfile::ReactionActivity
            ) | (
                MacroscopicProcess::ExplosiveMetalWater(_),
                EffectProfile::FlameEmitter(FlamePalette::Natural)
                    | EffectProfile::VapourRelease
                    | EffectProfile::SplashEmitter
                    | EffectProfile::HeatDistortion
            )
        );
    }
    if !matches!(
        authorization,
        EffectAuthorization::Observation(authorized) if authorized == predicate
    ) {
        return false;
    }
    match effect {
        EffectProfile::ReactionActivity
        | EffectProfile::SolidFormation
        | EffectProfile::PrecipitateFormation
        | EffectProfile::Clouding => matches!(predicate, ObservationPredicate::Forms),
        EffectProfile::BubbleEmitter | EffectProfile::GasRelease | EffectProfile::VapourRelease => {
            matches!(
                predicate,
                ObservationPredicate::Evolves | ObservationPredicate::Forms
            )
        }
        EffectProfile::FlameEmitter(_) => matches!(predicate, ObservationPredicate::Evolves),
        EffectProfile::ObjectShrinkage => {
            matches!(predicate, ObservationPredicate::Disappears)
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
        EffectProfile::SurfaceOxidation | EffectProfile::HeatDistortion => false,
    }
}

fn product_has_process_authorization(
    profile: &PresentationProfile,
    object: &PresentationObject,
) -> bool {
    profile.precipitation.as_ref().is_some_and(|precipitation| {
        precipitation.precipitate.binding == object.id
            && precipitation.formation_ordinal == object.visible_from_ordinal
            && object.asset == AssetProfile::PrecipitateCloud
            && precipitation_process_authorized(profile, precipitation.formation_ordinal)
    }) || profile.gas_evolution.as_ref().is_some_and(|gas_evolution| {
        gas_evolution.gas_product.binding == object.id
            && gas_evolution.generation_ordinal == object.visible_from_ordinal
            && object.asset == AssetProfile::GasCloud
            && profile.effects.iter().any(|effect| {
                effect.effect == EffectProfile::GasRelease
                    && effect.start_ordinal == gas_evolution.generation_ordinal
                    && matches!(
                        effect.authorization,
                        EffectAuthorization::Process(
                            MacroscopicProcess::GasEvolutionLiquidLiquid
                                | MacroscopicProcess::GasEvolutionSolidLiquid
                        )
                    )
            })
    })
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
        | AssetProfile::ReactiveMetalWaterAssembly
        | AssetProfile::ExplosiveMetalWaterAssembly
        | AssetProfile::NeutralisationEvaporationAssembly
        | AssetProfile::CompleteCombustionAssembly
        | AssetProfile::IncompleteCombustionAssembly
        | AssetProfile::AqueousPrecipitationAssembly
        | AssetProfile::MetalDisplacementAssembly
        | AssetProfile::SolidSolidSynthesisAssembly
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
    let mut beats = boundaries
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
                    stage: MacroscopicStage::Reaction,
                }
            })
        })
        .collect::<Vec<_>>();
    if profile.post_process == Some(MacroscopicProcess::SolventEvaporationCrystallization) {
        let camera = |stage, duration_ms| RealWorldBeat {
            start_ordinal: final_ordinal,
            end_ordinal: final_ordinal,
            duration_ms,
            camera: CameraCue {
                behaviour: CameraBehaviour::WideEstablishingShot,
                start_ordinal: final_ordinal,
                end_ordinal: final_ordinal,
            },
            stage,
        };
        beats.extend([
            camera(MacroscopicStage::HeatingPreparation, 1_200),
            camera(MacroscopicStage::SolventBoiling, 4_600),
            camera(MacroscopicStage::CrystalGrowth, 2_800),
        ]);
    }
    if let Some(precipitation) = &profile.precipitation {
        fit_authored_precipitation_duration(&mut beats, precipitation.formation_ordinal);
    }
    if profile.gas_evolution.is_some() {
        fit_authored_six_second_duration(&mut beats);
    }
    if profile.metal_displacement.is_some() {
        fit_authored_six_second_duration(&mut beats);
    }
    if profile.solid_solid_synthesis.is_some() {
        fit_authored_six_second_duration(&mut beats);
    }
    if profile.explosive_metal_water.is_some() {
        fit_authored_six_second_duration(&mut beats);
    }
    RealWorldTimeline { beats }
}

fn fit_authored_six_second_duration(beats: &mut [RealWorldBeat]) {
    // The authored clips hold 6 s of baked motion; presenting that window
    // over 9.6 s (0.625x speed) paces the reaction with the camera work
    // while 60 Hz interpolation keeps the motion smooth.
    const DURATION_MS: u32 = 9_600;
    let reaction_count = beats
        .iter()
        .take_while(|beat| beat.stage == MacroscopicStage::Reaction)
        .count();
    let source_total = beats[..reaction_count].iter().fold(0_u64, |total, beat| {
        total.saturating_add(u64::from(beat.duration_ms))
    });
    if reaction_count == 0 || source_total == 0 {
        return;
    }
    let mut assigned = 0_u32;
    for (index, beat) in beats[..reaction_count].iter_mut().enumerate() {
        let duration = if index + 1 == reaction_count {
            DURATION_MS.saturating_sub(assigned)
        } else {
            let scaled =
                u64::from(beat.duration_ms).saturating_mul(u64::from(DURATION_MS)) / source_total;
            u32::try_from(scaled).unwrap_or(DURATION_MS).max(1)
        };
        beat.duration_ms = duration;
        assigned = assigned.saturating_add(duration);
    }
}

fn fit_authored_precipitation_duration(beats: &mut [RealWorldBeat], formation_ordinal: u16) {
    // Same 0.625x presentation stretch as the shared authored window.
    const DURATION_MS: u32 = 9_600;
    let Some(start) = beats.iter().position(|beat| {
        beat.stage == MacroscopicStage::Reaction && beat.start_ordinal == formation_ordinal
    }) else {
        return;
    };
    let end = beats[start..]
        .iter()
        .position(|beat| beat.stage != MacroscopicStage::Reaction)
        .map_or(beats.len(), |offset| start + offset);
    let source_total = beats[start..end].iter().fold(0_u64, |total, beat| {
        total.saturating_add(u64::from(beat.duration_ms))
    });
    if source_total == 0 || start == end {
        return;
    }
    let mut assigned = 0_u32;
    let beat_count = end - start;
    for (offset, beat) in beats[start..end].iter_mut().enumerate() {
        let is_last = offset + 1 == beat_count;
        let duration = if is_last {
            DURATION_MS.saturating_sub(assigned)
        } else {
            let scaled =
                u64::from(beat.duration_ms).saturating_mul(u64::from(DURATION_MS)) / source_total;
            u32::try_from(scaled).unwrap_or(DURATION_MS).max(1)
        };
        beat.duration_ms = duration;
        assigned = assigned.saturating_add(duration);
    }
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
    // Paced for the choreographed camera (2026-07-19): beats breathe so a
    // push-in completes before its observation finishes, instead of the
    // whole reaction rushing past in a few seconds.
    let duration_ms = match intensity {
        Some(EffectIntensity::Strong) => 4_200,
        Some(EffectIntensity::Moderate) => 5_400,
        Some(EffectIntensity::Subtle) => 7_000,
        None if starts_at_initial_state => 1_400,
        None => 2_900,
    };
    if is_final {
        if duration_ms < 3_800 {
            3_800
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
        text: "The validated frame sequence has reached its reviewed outcome.".to_owned(),
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
    InvalidPrecipitationProfile,
    InvalidGasEvolutionProfile,
    InvalidMetalDisplacementProfile,
    InvalidSolidSolidSynthesisProfile,
    InvalidExplosiveMetalWaterProfile,
    PresentationRange,
    Digest,
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFrames => formatter.write_str("validated frames are absent"),
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
            Self::InvalidPrecipitationProfile => formatter.write_str(
                "precipitation assembly lacks exact validated formation and material bindings",
            ),
            Self::InvalidGasEvolutionProfile => formatter.write_str(
                "gas-evolution assembly lacks exact validated gas and material bindings",
            ),
            Self::InvalidMetalDisplacementProfile => formatter.write_str(
                "metal-displacement assembly lacks exact validated phase, identity, formation, or material bindings",
            ),
            Self::InvalidSolidSolidSynthesisProfile => formatter.write_str(
                "solid-solid synthesis assembly lacks exactly two solid reactants, one solid product, or validated formation and colour bindings",
            ),
            Self::InvalidExplosiveMetalWaterProfile => formatter.write_str(
                "high-energy metal/water assembly lacks exact reviewed variant, phase layout, effect, or material bindings",
            ),
            Self::PresentationRange => {
                formatter.write_str("validated frames exceed the presentation range")
            }
            Self::Digest => formatter.write_str("validated frame digest is unavailable"),
        }
    }
}

impl std::error::Error for PlanError {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chem_catalogue::ObservationPredicate;
    use chem_domain::{Phase, RepresentationKind};

    use super::{
        AppearanceProfile, AssetProfile, EducationalPlan, EducationalScene, EducationalSceneKind,
        EffectAuthorization, EffectIntensity, EffectProfile, FlamePalette, MacroscopicMaterial,
        MacroscopicMaterialRole, MacroscopicProcess, MacroscopicReaction, MacroscopicStage,
        ObjectObservationBinding, PresentationEffect, PresentationObject, PresentationProfile,
        PresentationTransform, ReactionVisualInputs, SceneRole, TimelinePosition, VisualColour,
        authorize_explosive_metal_water_assembly, authorize_gas_evolution_assembly,
        authorize_metal_displacement_assembly, authorize_solid_solid_synthesis_assembly,
        compile_real_world_timeline, effect_authorization_is_compatible,
        electrolysis_transfer_text, macroscopic_beat_duration_ms,
        precipitation_colours_from_materials, visual_colour,
    };

    fn precipitation_material(
        binding: &str,
        role: MacroscopicMaterialRole,
        phase: Phase,
        colour: Option<VisualColour>,
    ) -> MacroscopicMaterial {
        MacroscopicMaterial {
            binding: binding.to_owned(),
            semantic_identity: binding.to_owned(),
            structure_id: format!("Structures.{binding}"),
            formula: binding.to_owned(),
            role,
            phase,
            representation: RepresentationKind::Ionic,
            colour,
            explosive_water_contact: None,
        }
    }

    fn precipitation_product() -> PresentationObject {
        PresentationObject {
            id: "solid-product".to_owned(),
            asset: AssetProfile::PrecipitateCloud,
            semantic_identity: "validated solid product".to_owned(),
            appearance: AppearanceProfile::LaboratoryNeutral,
            role: SceneRole::Product,
            transform: PresentationTransform {
                translation: [0, 0, 0],
                rotation: [0, 0, 0],
                scale: [1_000, 1_000, 1_000],
            },
            visible_from_ordinal: 4,
            observation: Some(ObjectObservationBinding {
                predicate: ObservationPredicate::Forms,
                value: None,
            }),
            colour_transition: None,
        }
    }

    fn gas_evolution_profile(predicate: ObservationPredicate) -> PresentationProfile {
        PresentationProfile {
            id: "generic-gas-evolution".to_owned(),
            environment: AssetProfile::LaboratoryBench,
            objects: vec![
                PresentationObject {
                    id: "vessel".to_owned(),
                    asset: AssetProfile::Beaker,
                    semantic_identity: "open vessel".to_owned(),
                    appearance: AppearanceProfile::ClearGlass,
                    role: SceneRole::Vessel,
                    transform: PresentationTransform {
                        translation: [0, 0, 0],
                        rotation: [0, 0, 0],
                        scale: [1_000, 1_000, 1_000],
                    },
                    visible_from_ordinal: 0,
                    observation: None,
                    colour_transition: None,
                },
                PresentationObject {
                    id: "gas-product".to_owned(),
                    asset: AssetProfile::GasCloud,
                    semantic_identity: "validated gas product".to_owned(),
                    appearance: AppearanceProfile::LaboratoryNeutral,
                    role: SceneRole::Product,
                    transform: PresentationTransform {
                        translation: [0, 0, 0],
                        rotation: [0, 0, 0],
                        scale: [1_000, 1_000, 1_000],
                    },
                    visible_from_ordinal: 4,
                    observation: Some(ObjectObservationBinding {
                        predicate,
                        value: None,
                    }),
                    colour_transition: None,
                },
            ],
            effects: vec![PresentationEffect {
                effect: EffectProfile::GasRelease,
                trigger: predicate,
                authorization: EffectAuthorization::Observation(predicate),
                intensity: EffectIntensity::Moderate,
                start_ordinal: 4,
                end_ordinal: 6,
                surface_oxide_colour: None,
            }],
            camera: Vec::new(),
            precipitation: None,
            gas_evolution: None,
            metal_displacement: None,
            solid_solid_synthesis: None,
            explosive_metal_water: None,
            post_process: None,
            equation: "validated equation".to_owned(),
            disclosure: super::VIRTUAL_ONLY_DISCLOSURE.to_owned(),
        }
    }

    fn gas_reaction(
        first_phase: Phase,
        second_phase: Phase,
        process: Option<MacroscopicProcess>,
        colours: [Option<VisualColour>; 3],
    ) -> MacroscopicReaction {
        MacroscopicReaction {
            profile_id: "generic-gas-evolution".to_owned(),
            equation: "validated equation".to_owned(),
            materials: vec![
                precipitation_material(
                    "first-reactant",
                    MacroscopicMaterialRole::Reactant,
                    first_phase,
                    colours[0],
                ),
                precipitation_material(
                    "second-reactant",
                    MacroscopicMaterialRole::Reactant,
                    second_phase,
                    colours[1],
                ),
                precipitation_material(
                    "gas-product",
                    MacroscopicMaterialRole::Product,
                    Phase::Gas,
                    colours[2],
                ),
            ],
            intensity: EffectIntensity::Moderate,
            process,
            fuel_carbon_count: None,
            surface_oxide_colour: None,
        }
    }

    fn gas_observations() -> BTreeMap<(String, ObservationPredicate), (u16, Option<String>)> {
        BTreeMap::from([(
            ("gas-product".to_owned(), ObservationPredicate::Evolves),
            (4, None),
        )])
    }

    fn explosive_material(
        binding: &str,
        role: MacroscopicMaterialRole,
        phase: Phase,
        representation: RepresentationKind,
        capability: Option<super::ExplosiveMetalWaterVariant>,
    ) -> MacroscopicMaterial {
        MacroscopicMaterial {
            binding: binding.to_owned(),
            semantic_identity: binding.to_owned(),
            structure_id: format!("Structures.{binding}"),
            formula: binding.to_owned(),
            role,
            phase,
            representation,
            colour: None,
            explosive_water_contact: capability,
        }
    }

    fn explosive_profile(variant: super::ExplosiveMetalWaterVariant) -> PresentationProfile {
        let process = MacroscopicProcess::ExplosiveMetalWater(variant);
        PresentationProfile {
            id: "generic-explosive-metal-water".to_owned(),
            environment: AssetProfile::LaboratoryBench,
            objects: vec![
                PresentationObject {
                    id: "vessel".to_owned(),
                    asset: AssetProfile::Beaker,
                    semantic_identity: "open vessel".to_owned(),
                    appearance: AppearanceProfile::ClearGlass,
                    role: SceneRole::Vessel,
                    transform: PresentationTransform {
                        translation: [0, 0, 0],
                        rotation: [0, 0, 0],
                        scale: [1_000, 1_000, 1_000],
                    },
                    visible_from_ordinal: 0,
                    observation: None,
                    colour_transition: None,
                },
                PresentationObject {
                    id: "hydrogen".to_owned(),
                    asset: AssetProfile::GasCloud,
                    semantic_identity: "validated gas product".to_owned(),
                    appearance: AppearanceProfile::LaboratoryNeutral,
                    role: SceneRole::Product,
                    transform: PresentationTransform {
                        translation: [0, 0, 0],
                        rotation: [0, 0, 0],
                        scale: [1_000, 1_000, 1_000],
                    },
                    visible_from_ordinal: 4,
                    observation: None,
                    colour_transition: None,
                },
            ],
            effects: [
                EffectProfile::FlameEmitter(FlamePalette::Natural),
                EffectProfile::VapourRelease,
                EffectProfile::SplashEmitter,
                EffectProfile::HeatDistortion,
            ]
            .into_iter()
            .map(|effect| PresentationEffect {
                effect,
                trigger: ObservationPredicate::Forms,
                authorization: EffectAuthorization::Process(process),
                intensity: EffectIntensity::Strong,
                start_ordinal: 4,
                end_ordinal: 6,
                surface_oxide_colour: None,
            })
            .collect(),
            camera: Vec::new(),
            precipitation: None,
            gas_evolution: None,
            metal_displacement: None,
            solid_solid_synthesis: None,
            explosive_metal_water: None,
            post_process: None,
            equation: "validated equation".to_owned(),
            disclosure: super::VIRTUAL_ONLY_DISCLOSURE.to_owned(),
        }
    }

    fn explosive_reaction(
        variant: super::ExplosiveMetalWaterVariant,
        materials: Vec<MacroscopicMaterial>,
        process: Option<MacroscopicProcess>,
    ) -> MacroscopicReaction {
        MacroscopicReaction {
            profile_id: "generic-explosive-metal-water".to_owned(),
            equation: "validated equation".to_owned(),
            materials,
            intensity: EffectIntensity::Strong,
            process: process.or(Some(MacroscopicProcess::ExplosiveMetalWater(variant))),
            fuel_carbon_count: None,
            surface_oxide_colour: None,
        }
    }

    #[test]
    fn explosive_metal_water_selection_requires_all_exact_typed_materials() {
        for variant in [
            super::ExplosiveMetalWaterVariant::Rubidium,
            super::ExplosiveMetalWaterVariant::Caesium,
            super::ExplosiveMetalWaterVariant::Francium,
        ] {
            let reaction = explosive_reaction(
                variant,
                vec![
                    explosive_material(
                        "metal",
                        MacroscopicMaterialRole::Reactant,
                        Phase::Solid,
                        RepresentationKind::Metallic,
                        Some(variant),
                    ),
                    explosive_material(
                        "water",
                        MacroscopicMaterialRole::Reactant,
                        Phase::Liquid,
                        RepresentationKind::Molecular,
                        None,
                    ),
                    explosive_material(
                        "hydroxide",
                        MacroscopicMaterialRole::Product,
                        Phase::Aqueous,
                        RepresentationKind::Ionic,
                        None,
                    ),
                    explosive_material(
                        "hydrogen",
                        MacroscopicMaterialRole::Product,
                        Phase::Gas,
                        RepresentationKind::Molecular,
                        None,
                    ),
                ],
                None,
            );
            let mut profile = explosive_profile(variant);
            authorize_explosive_metal_water_assembly(&mut profile, &reaction, &BTreeMap::new());
            assert_eq!(
                profile
                    .explosive_metal_water
                    .as_ref()
                    .map(|visual| visual.variant),
                Some(variant)
            );
        }
    }

    #[test]
    fn explosive_metal_water_rejects_missing_ambiguous_and_lower_priority_layouts() {
        let variant = super::ExplosiveMetalWaterVariant::Rubidium;
        let exact = vec![
            explosive_material(
                "metal",
                MacroscopicMaterialRole::Reactant,
                Phase::Solid,
                RepresentationKind::Metallic,
                Some(variant),
            ),
            explosive_material(
                "water",
                MacroscopicMaterialRole::Reactant,
                Phase::Liquid,
                RepresentationKind::Molecular,
                None,
            ),
            explosive_material(
                "hydroxide",
                MacroscopicMaterialRole::Product,
                Phase::Aqueous,
                RepresentationKind::Ionic,
                None,
            ),
            explosive_material(
                "hydrogen",
                MacroscopicMaterialRole::Product,
                Phase::Gas,
                RepresentationKind::Molecular,
                None,
            ),
        ];
        let scenarios = [
            (
                exact
                    .iter()
                    .filter(|material| material.binding != "water")
                    .cloned()
                    .collect(),
                Some(MacroscopicProcess::ExplosiveMetalWater(variant)),
            ),
            (
                {
                    let mut materials = exact.clone();
                    materials.push(explosive_material(
                        "extra-product",
                        MacroscopicMaterialRole::Product,
                        Phase::Gas,
                        RepresentationKind::Molecular,
                        None,
                    ));
                    materials
                },
                Some(MacroscopicProcess::ExplosiveMetalWater(variant)),
            ),
            (exact, Some(MacroscopicProcess::MetalDisplacement)),
        ];
        for (materials, process) in scenarios {
            let reaction = explosive_reaction(variant, materials, process);
            let mut profile = explosive_profile(variant);
            authorize_explosive_metal_water_assembly(&mut profile, &reaction, &BTreeMap::new());
            assert!(profile.explosive_metal_water.is_none());
        }
    }

    fn solid_synthesis_profile(include_front: bool) -> PresentationProfile {
        let mut effects = vec![PresentationEffect {
            effect: EffectProfile::SolidFormation,
            trigger: ObservationPredicate::Forms,
            authorization: EffectAuthorization::Process(MacroscopicProcess::SolidSolidSynthesis),
            intensity: EffectIntensity::Moderate,
            start_ordinal: 4,
            end_ordinal: 6,
            surface_oxide_colour: None,
        }];
        if include_front {
            effects.push(PresentationEffect {
                effect: EffectProfile::ReactionActivity,
                trigger: ObservationPredicate::Forms,
                authorization: EffectAuthorization::Process(
                    MacroscopicProcess::SolidSolidSynthesis,
                ),
                intensity: EffectIntensity::Moderate,
                start_ordinal: 4,
                end_ordinal: 6,
                surface_oxide_colour: None,
            });
        }
        PresentationProfile {
            id: "solid-solid-synthesis".to_owned(),
            environment: AssetProfile::LaboratoryBench,
            objects: vec![PresentationObject {
                id: "vessel".to_owned(),
                asset: AssetProfile::Beaker,
                semantic_identity: "reaction vessel".to_owned(),
                appearance: AppearanceProfile::LaboratoryNeutral,
                role: SceneRole::Vessel,
                transform: PresentationTransform {
                    translation: [0, 0, 0],
                    rotation: [0, 0, 0],
                    scale: [1_000, 1_000, 1_000],
                },
                visible_from_ordinal: 0,
                observation: None,
                colour_transition: None,
            }],
            effects,
            camera: Vec::new(),
            precipitation: None,
            gas_evolution: None,
            metal_displacement: None,
            solid_solid_synthesis: None,
            explosive_metal_water: None,
            post_process: None,
            equation: "validated equation".to_owned(),
            disclosure: super::VIRTUAL_ONLY_DISCLOSURE.to_owned(),
        }
    }

    fn solid_synthesis_reaction(materials: Vec<MacroscopicMaterial>) -> MacroscopicReaction {
        MacroscopicReaction {
            profile_id: "solid-solid-synthesis".to_owned(),
            equation: "validated equation".to_owned(),
            materials,
            intensity: EffectIntensity::Moderate,
            process: Some(MacroscopicProcess::SolidSolidSynthesis),
            fuel_carbon_count: None,
            surface_oxide_colour: None,
        }
    }

    #[test]
    fn solid_solid_synthesis_uses_exact_catalogue_colours_and_optional_front() {
        let colours = [
            VisualColour {
                red: 0x88,
                green: 0x8c,
                blue: 0x90,
            },
            VisualColour {
                red: 0xe7,
                green: 0xc5,
                blue: 0x32,
            },
            VisualColour {
                red: 0x31,
                green: 0x34,
                blue: 0x37,
            },
        ];
        let reaction = solid_synthesis_reaction(vec![
            precipitation_material(
                "reactant-a",
                MacroscopicMaterialRole::Reactant,
                Phase::Solid,
                Some(colours[0]),
            ),
            precipitation_material(
                "reactant-b",
                MacroscopicMaterialRole::Reactant,
                Phase::Solid,
                Some(colours[1]),
            ),
            precipitation_material(
                "product",
                MacroscopicMaterialRole::Product,
                Phase::Solid,
                Some(colours[2]),
            ),
        ]);
        let mut profile = solid_synthesis_profile(false);
        authorize_solid_solid_synthesis_assembly(&mut profile, &reaction, &BTreeMap::new());
        let synthesis = profile
            .solid_solid_synthesis
            .as_ref()
            .expect("typed solid layout selects authored synthesis");
        assert_eq!(synthesis.reactant_a.colour, colours[0]);
        assert_eq!(synthesis.reactant_b.colour, colours[1]);
        assert_eq!(synthesis.product.colour, colours[2]);
        assert!(!synthesis.show_reaction_front);
        assert!(
            profile
                .objects
                .iter()
                .any(|object| { object.asset == AssetProfile::SolidSolidSynthesisAssembly })
        );
        assert_eq!(
            compile_real_world_timeline(&profile, 6).duration_ms(),
            9_600
        );
    }

    #[test]
    fn solid_solid_synthesis_rejects_extra_or_ambiguous_reactants() {
        let solid = |binding, role| precipitation_material(binding, role, Phase::Solid, None);
        let mut extra_profile = solid_synthesis_profile(true);
        let extra = solid_synthesis_reaction(vec![
            solid("a", MacroscopicMaterialRole::Reactant),
            solid("b", MacroscopicMaterialRole::Reactant),
            solid("c", MacroscopicMaterialRole::Reactant),
            solid("product", MacroscopicMaterialRole::Product),
        ]);
        authorize_solid_solid_synthesis_assembly(&mut extra_profile, &extra, &BTreeMap::new());
        assert!(extra_profile.solid_solid_synthesis.is_none());

        let mut ambiguous_profile = solid_synthesis_profile(true);
        let ambiguous = solid_synthesis_reaction(vec![
            solid("a", MacroscopicMaterialRole::Reactant),
            precipitation_material("b", MacroscopicMaterialRole::Reactant, Phase::Unknown, None),
            solid("product", MacroscopicMaterialRole::Product),
        ]);
        authorize_solid_solid_synthesis_assembly(
            &mut ambiguous_profile,
            &ambiguous,
            &BTreeMap::new(),
        );
        assert!(ambiguous_profile.solid_solid_synthesis.is_none());
    }

    #[test]
    fn solid_solid_synthesis_uses_conservative_missing_colour_fallbacks() {
        let reaction = solid_synthesis_reaction(vec![
            precipitation_material(
                "reactant-a",
                MacroscopicMaterialRole::Reactant,
                Phase::Solid,
                None,
            ),
            precipitation_material(
                "reactant-b",
                MacroscopicMaterialRole::Reactant,
                Phase::Solid,
                None,
            ),
            precipitation_material(
                "product",
                MacroscopicMaterialRole::Product,
                Phase::Solid,
                None,
            ),
        ]);
        let mut profile = solid_synthesis_profile(true);
        authorize_solid_solid_synthesis_assembly(&mut profile, &reaction, &BTreeMap::new());
        let synthesis = profile
            .solid_solid_synthesis
            .expect("complete typed layout selects synthesis");
        assert_eq!(synthesis.reactant_a.colour, super::NEUTRAL_METAL);
        assert_eq!(synthesis.reactant_b.colour, super::OFF_WHITE_PRECIPITATE);
        assert_eq!(synthesis.product.colour, super::NEUTRAL_DEPOSITED_METAL);
        assert!(synthesis.show_reaction_front);
    }

    #[test]
    fn liquid_liquid_gas_evolution_uses_exact_catalogue_colour_bindings() {
        let first = VisualColour {
            red: 0x32,
            green: 0x73,
            blue: 0xa5,
        };
        let second = VisualColour {
            red: 0xd6,
            green: 0xb4,
            blue: 0x58,
        };
        let gas = VisualColour {
            red: 0xb4,
            green: 0xd7,
            blue: 0x71,
        };
        let reaction = gas_reaction(
            Phase::Aqueous,
            Phase::Liquid,
            None,
            [Some(first), Some(second), Some(gas)],
        );
        let mut profile = gas_evolution_profile(ObservationPredicate::Evolves);
        authorize_gas_evolution_assembly(&mut profile, &reaction, &gas_observations());
        let visual = profile
            .gas_evolution
            .expect("two mobile reactants select liquid-liquid gas evolution");
        assert_eq!(visual.variant, super::GasEvolutionVariant::LiquidLiquid);
        assert_eq!(visual.initial_reactant.colour, first);
        assert_eq!(visual.added_reactant.colour, second);
        assert_eq!(visual.gas_product.colour, gas);
    }

    #[test]
    fn solid_liquid_gas_evolution_selects_by_phase_in_either_reactant_order() {
        for phases in [
            (Phase::Solid, Phase::Aqueous),
            (Phase::Liquid, Phase::Solid),
        ] {
            let reaction = gas_reaction(phases.0, phases.1, None, [None, None, None]);
            let mut profile = gas_evolution_profile(ObservationPredicate::Evolves);
            authorize_gas_evolution_assembly(&mut profile, &reaction, &gas_observations());
            assert_eq!(
                profile.gas_evolution.as_ref().map(|visual| visual.variant),
                Some(super::GasEvolutionVariant::SolidLiquid)
            );
        }
    }

    #[test]
    fn combustion_and_ambiguous_phases_retain_the_existing_animation_selection() {
        let mut combustion = gas_evolution_profile(ObservationPredicate::Evolves);
        authorize_gas_evolution_assembly(
            &mut combustion,
            &gas_reaction(
                Phase::Liquid,
                Phase::Aqueous,
                Some(MacroscopicProcess::CompleteCombustion),
                [None, None, None],
            ),
            &gas_observations(),
        );
        assert!(combustion.gas_evolution.is_none());

        let mut ambiguous = gas_evolution_profile(ObservationPredicate::Evolves);
        authorize_gas_evolution_assembly(
            &mut ambiguous,
            &gas_reaction(Phase::Unknown, Phase::Liquid, None, [None, None, None]),
            &gas_observations(),
        );
        assert!(ambiguous.gas_evolution.is_none());
    }

    #[test]
    fn colourless_gas_and_uncoloured_reactants_use_conservative_fallbacks() {
        let reaction = gas_reaction(Phase::Liquid, Phase::Aqueous, None, [None, None, None]);
        let mut profile = gas_evolution_profile(ObservationPredicate::Evolves);
        authorize_gas_evolution_assembly(&mut profile, &reaction, &gas_observations());
        let visual = profile
            .gas_evolution
            .expect("supported phases still select with missing optional colours");
        assert_eq!(visual.initial_reactant.colour, super::COLOURLESS_LIQUID);
        assert_eq!(visual.added_reactant.colour, super::COLOURLESS_LIQUID);
        assert_eq!(visual.gas_product.colour, super::PALE_COLOURLESS_GAS);
    }

    fn displacement_reaction(
        phases: [Phase; 4],
        colours: [Option<VisualColour>; 4],
    ) -> MacroscopicReaction {
        let material = |binding: &str,
                        role: MacroscopicMaterialRole,
                        phase: Phase,
                        representation: RepresentationKind,
                        colour: Option<VisualColour>|
         -> MacroscopicMaterial {
            MacroscopicMaterial {
                binding: binding.to_owned(),
                semantic_identity: binding.to_owned(),
                structure_id: format!("Structures.{binding}"),
                formula: binding.to_owned(),
                role,
                phase,
                representation,
                colour,
                explosive_water_contact: None,
            }
        };
        MacroscopicReaction {
            profile_id: "generic-metal-displacement".to_owned(),
            equation: "validated equation".to_owned(),
            materials: vec![
                material(
                    "original-metal",
                    MacroscopicMaterialRole::Reactant,
                    phases[0],
                    RepresentationKind::Metallic,
                    colours[0],
                ),
                material(
                    "initial-solution",
                    MacroscopicMaterialRole::Reactant,
                    phases[1],
                    RepresentationKind::Ionic,
                    colours[1],
                ),
                material(
                    "final-solution",
                    MacroscopicMaterialRole::Product,
                    phases[2],
                    RepresentationKind::Ionic,
                    colours[2],
                ),
                material(
                    "deposited-metal",
                    MacroscopicMaterialRole::Product,
                    phases[3],
                    RepresentationKind::Metallic,
                    colours[3],
                ),
            ],
            intensity: EffectIntensity::Moderate,
            process: Some(MacroscopicProcess::MetalDisplacement),
            fuel_carbon_count: None,
            surface_oxide_colour: None,
        }
    }

    fn displacement_profile() -> PresentationProfile {
        let mut profile = gas_evolution_profile(ObservationPredicate::Forms);
        profile.objects.truncate(1);
        profile.effects = vec![PresentationEffect {
            effect: EffectProfile::SolidFormation,
            trigger: ObservationPredicate::Forms,
            authorization: EffectAuthorization::Process(MacroscopicProcess::MetalDisplacement),
            intensity: EffectIntensity::Moderate,
            start_ordinal: 3,
            end_ordinal: 5,
            surface_oxide_colour: None,
        }];
        profile
    }

    #[test]
    fn metal_displacement_uses_exact_role_colours_and_conservative_fallbacks() {
        let initial = VisualColour {
            red: 0x43,
            green: 0x79,
            blue: 0xb2,
        };
        let deposited = VisualColour {
            red: 0xb8,
            green: 0x69,
            blue: 0x47,
        };
        let reaction = displacement_reaction(
            [Phase::Solid, Phase::Aqueous, Phase::Aqueous, Phase::Solid],
            [None, Some(initial), None, Some(deposited)],
        );
        let mut profile = displacement_profile();
        authorize_metal_displacement_assembly(&mut profile, &reaction, &BTreeMap::new());
        let visual = profile
            .metal_displacement
            .as_ref()
            .expect("validated cross-side process selects authored displacement");
        assert_eq!(visual.initial_solution.colour, initial);
        assert_eq!(visual.final_solution.colour, super::COLOURLESS_LIQUID);
        assert_eq!(visual.original_metal.colour, super::NEUTRAL_METAL);
        assert_eq!(visual.deposited_metal.colour, deposited);
        assert!(profile.objects.iter().any(|object| {
            object.asset == AssetProfile::MetalDisplacementAssembly
                && object.role == SceneRole::Vessel
        }));
        assert_eq!(
            compile_real_world_timeline(&profile, 5)
                .beats
                .iter()
                .filter(|beat| beat.stage == MacroscopicStage::Reaction)
                .map(|beat| beat.duration_ms)
                .sum::<u32>(),
            9_600
        );
    }

    #[test]
    fn metal_displacement_rejects_ambiguous_phase_layouts() {
        let reaction = displacement_reaction(
            [Phase::Unknown, Phase::Aqueous, Phase::Aqueous, Phase::Solid],
            [None; 4],
        );
        let mut profile = displacement_profile();
        authorize_metal_displacement_assembly(&mut profile, &reaction, &BTreeMap::new());
        assert!(profile.metal_displacement.is_none());
        assert!(
            profile
                .objects
                .iter()
                .all(|object| { object.asset != AssetProfile::MetalDisplacementAssembly })
        );
    }

    #[test]
    fn precipitation_colours_use_reviewed_catalogue_rgb_by_exact_role_binding() {
        let initial = VisualColour {
            red: 0x45,
            green: 0x74,
            blue: 0xa8,
        };
        let added = VisualColour {
            red: 0xd7,
            green: 0xb6,
            blue: 0x63,
        };
        let solid = VisualColour {
            red: 0x84,
            green: 0x5b,
            blue: 0xa6,
        };
        let reaction = MacroscopicReaction {
            profile_id: "generic-precipitation".to_owned(),
            equation: "validated equation".to_owned(),
            materials: vec![
                precipitation_material(
                    "initial-solution",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Aqueous,
                    Some(initial),
                ),
                precipitation_material(
                    "added-solution",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Aqueous,
                    Some(added),
                ),
                precipitation_material(
                    "solid-product",
                    MacroscopicMaterialRole::Product,
                    Phase::Solid,
                    Some(solid),
                ),
            ],
            intensity: EffectIntensity::Moderate,
            process: None,
            fuel_carbon_count: None,
            surface_oxide_colour: None,
        };
        let colours = precipitation_colours_from_materials(
            &reaction,
            &BTreeMap::new(),
            &precipitation_product(),
            4,
        )
        .expect("two reviewed mobile phases and a solid authorize colour bindings");
        assert_eq!(colours.initial_liquid.colour, initial);
        assert_eq!(colours.added_liquid.colour, added);
        assert_eq!(colours.precipitate.colour, solid);
    }

    #[test]
    fn precipitation_colours_have_conservative_missing_colour_fallbacks() {
        let reaction = MacroscopicReaction {
            profile_id: "generic-precipitation".to_owned(),
            equation: "validated equation".to_owned(),
            materials: vec![
                precipitation_material(
                    "initial-solution",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Liquid,
                    None,
                ),
                precipitation_material(
                    "added-solution",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Aqueous,
                    None,
                ),
                precipitation_material(
                    "solid-product",
                    MacroscopicMaterialRole::Product,
                    Phase::Solid,
                    None,
                ),
            ],
            intensity: EffectIntensity::Moderate,
            process: None,
            fuel_carbon_count: None,
            surface_oxide_colour: None,
        };
        let colours = precipitation_colours_from_materials(
            &reaction,
            &BTreeMap::new(),
            &precipitation_product(),
            4,
        )
        .expect("missing optional colours retain conservative defaults");
        assert_eq!(colours.initial_liquid.colour, super::COLOURLESS_LIQUID);
        assert_eq!(colours.added_liquid.colour, super::COLOURLESS_LIQUID);
        assert_eq!(colours.precipitate.colour, super::OFF_WHITE_PRECIPITATE);
    }

    #[test]
    fn exact_colour_observation_outranks_reviewed_precipitate_catalogue_rgb() {
        let reviewed = VisualColour {
            red: 0x20,
            green: 0x55,
            blue: 0x90,
        };
        let exact = visual_colour("Rgb.HexD08A42").expect("exact colour syntax resolves");
        let reaction = MacroscopicReaction {
            profile_id: "generic-precipitation".to_owned(),
            equation: "validated equation".to_owned(),
            materials: vec![
                precipitation_material(
                    "initial-solution",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Aqueous,
                    None,
                ),
                precipitation_material(
                    "added-solution",
                    MacroscopicMaterialRole::Reactant,
                    Phase::Aqueous,
                    None,
                ),
                precipitation_material(
                    "solid-product",
                    MacroscopicMaterialRole::Product,
                    Phase::Solid,
                    Some(reviewed),
                ),
            ],
            intensity: EffectIntensity::Moderate,
            process: None,
            fuel_carbon_count: None,
            surface_oxide_colour: None,
        };
        let active = BTreeMap::from([(
            ("solid-product".to_owned(), ObservationPredicate::Colour),
            (5, Some("Rgb.HexD08A42".to_owned())),
        )]);
        let colours =
            precipitation_colours_from_materials(&reaction, &active, &precipitation_product(), 4)
                .expect("exact observation colour resolves");
        assert_eq!(colours.precipitate.colour, exact);
        assert_ne!(colours.precipitate.colour, reviewed);
    }

    #[test]
    fn electrolysis_transfer_copy_uses_electrodes_not_direct_ion_motion() {
        let (anode, cathode) = electrolysis_transfer_text("Cl", "Ag", 1, "electron");
        assert_eq!(anode, "Anode: Cl transfers 1 electron");
        assert!(cathode.starts_with("Cathode: Ag receives 1 electron."));
        assert!(cathode.contains("external circuit"));
        assert!(cathode.contains("not directly between these ions"));
        assert!(!cathode.contains("jumps"));
    }

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
                    kind: EducationalSceneKind::ReactantSetup,
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
    fn normalized_real_world_progress_is_linear_across_unequal_beats() {
        let cue = super::CameraCue {
            behaviour: super::CameraBehaviour::WideEstablishingShot,
            start_ordinal: 0,
            end_ordinal: 0,
        };
        let timeline = super::RealWorldTimeline {
            beats: vec![
                super::RealWorldBeat {
                    start_ordinal: 0,
                    end_ordinal: 0,
                    duration_ms: 1_000,
                    camera: cue.clone(),
                    stage: MacroscopicStage::Reaction,
                },
                super::RealWorldBeat {
                    start_ordinal: 1,
                    end_ordinal: 2,
                    duration_ms: 3_000,
                    camera: cue,
                    stage: MacroscopicStage::Reaction,
                },
            ],
        };
        for (elapsed, expected) in [(0, 0.0), (1_000, 0.25), (2_000, 0.50), (3_000, 0.75)] {
            let position = timeline.locate(elapsed).expect("time is inside timeline");
            assert!(
                (timeline.normalized_progress_at(position) - expected).abs() < 0.000_1,
                "{elapsed} ms should map to {expected}"
            );
        }
        let completed = timeline
            .locate(timeline.duration_ms())
            .expect("timeline endpoint exists");
        assert!((timeline.normalized_progress_at(completed) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn solvent_separation_appends_distinct_post_reaction_beats() {
        let profile = PresentationProfile {
            id: "generic-solvent-separation".to_owned(),
            environment: AssetProfile::LaboratoryBench,
            objects: Vec::new(),
            effects: Vec::new(),
            camera: Vec::new(),
            precipitation: None,
            gas_evolution: None,
            metal_displacement: None,
            solid_solid_synthesis: None,
            explosive_metal_water: None,
            post_process: Some(MacroscopicProcess::SolventEvaporationCrystallization),
            equation: "validated reaction".to_owned(),
            disclosure: super::VIRTUAL_ONLY_DISCLOSURE.to_owned(),
        };
        let timeline = compile_real_world_timeline(&profile, 4);
        let stages = timeline
            .beats
            .iter()
            .map(|beat| beat.stage)
            .collect::<Vec<_>>();
        assert!(stages.starts_with(&[MacroscopicStage::Reaction]));
        assert_eq!(
            &stages[stages.len() - 3..],
            &[
                MacroscopicStage::HeatingPreparation,
                MacroscopicStage::SolventBoiling,
                MacroscopicStage::CrystalGrowth,
            ]
        );
        assert_eq!(
            timeline
                .locate(timeline.duration_ms())
                .map(|position| position.stage),
            Some(MacroscopicStage::CrystalGrowth)
        );
    }

    #[test]
    fn visual_inputs_are_inferred_from_typed_effects_without_reaction_identity() {
        let effects = vec![
            PresentationEffect {
                effect: EffectProfile::BubbleEmitter,
                trigger: ObservationPredicate::Evolves,
                authorization: EffectAuthorization::Observation(ObservationPredicate::Evolves),
                intensity: EffectIntensity::Moderate,
                start_ordinal: 2,
                end_ordinal: 6,
                surface_oxide_colour: None,
            },
            PresentationEffect {
                effect: EffectProfile::GasRelease,
                trigger: ObservationPredicate::Evolves,
                authorization: EffectAuthorization::Observation(ObservationPredicate::Evolves),
                intensity: EffectIntensity::Moderate,
                start_ordinal: 2,
                end_ordinal: 6,
                surface_oxide_colour: None,
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
            authorization: EffectAuthorization::Observation(ObservationPredicate::Evolves),
            intensity: EffectIntensity::Strong,
            start_ordinal: 2,
            end_ordinal: 6,
            surface_oxide_colour: None,
        }];
        let inputs = ReactionVisualInputs::from_effects(&effects, 4, 0.5, 8);

        assert!(inputs.flame_rate > 0.9);
        assert!(inputs.container_vibration > 0.10);
        assert!(inputs.heat_output > 0.0);
        assert!(inputs.liquid_turbulence > 0.0);
        assert!((inputs.reaction_rate - inputs.flame_rate).abs() < f32::EPSILON);
    }

    #[test]
    fn validated_combustion_authorizes_natural_flame_and_hot_vapour_channels() {
        let process = MacroscopicProcess::CompleteCombustion;
        let effects = [
            PresentationEffect {
                effect: EffectProfile::FlameEmitter(FlamePalette::Natural),
                trigger: ObservationPredicate::Forms,
                authorization: EffectAuthorization::Process(process),
                intensity: EffectIntensity::Strong,
                start_ordinal: 2,
                end_ordinal: 6,
                surface_oxide_colour: None,
            },
            PresentationEffect {
                effect: EffectProfile::VapourRelease,
                trigger: ObservationPredicate::Forms,
                authorization: EffectAuthorization::Process(process),
                intensity: EffectIntensity::Strong,
                start_ordinal: 2,
                end_ordinal: 6,
                surface_oxide_colour: None,
            },
        ];
        assert!(effects.iter().all(|effect| {
            effect_authorization_is_compatible(effect.effect, effect.trigger, effect.authorization)
        }));
        assert!(!effect_authorization_is_compatible(
            EffectProfile::FlameEmitter(FlamePalette::Lilac),
            ObservationPredicate::Forms,
            EffectAuthorization::Process(process),
        ));
        assert!(!effect_authorization_is_compatible(
            EffectProfile::GasRelease,
            ObservationPredicate::Forms,
            EffectAuthorization::Process(process),
        ));

        let inputs = ReactionVisualInputs::from_effects(&effects, 4, 0.5, 8);
        assert!(inputs.flame_rate > 0.9);
        assert!(inputs.vapour_generation_rate > 0.9);
        assert!(inputs.gas_generation_rate > 0.5);
        assert!(inputs.heat_output > 0.9);
    }

    #[test]
    fn hydrocarbon_fuel_palette_uses_exact_carbon_count_boundaries() {
        let colour = |count| super::hydrocarbon_fuel_colour(count);
        assert_eq!(colour(1), colour(4));
        assert_ne!(colour(4), colour(5));
        assert_eq!(colour(5), colour(8));
        assert_ne!(colour(8), colour(9));
        assert_eq!(colour(9), colour(12));
        assert_ne!(colour(12), colour(13));
        assert_eq!(colour(13), colour(16));
        assert_ne!(colour(16), colour(17));
        assert_eq!(colour(17), colour(u64::MAX));
        assert_eq!(
            [colour(1).red, colour(1).green, colour(1).blue],
            [0xee, 0xef, 0xe8]
        );
        assert_eq!(
            [colour(17).red, colour(17).green, colour(17).blue],
            [0x4f, 0x2d, 0x1d]
        );
    }

    #[test]
    fn liquid_mixing_drives_flow_without_inventing_gas_or_solid_products() {
        let effects = [PresentationEffect {
            effect: EffectProfile::LiquidMixing,
            trigger: ObservationPredicate::Disappears,
            authorization: EffectAuthorization::Observation(ObservationPredicate::Disappears),
            intensity: EffectIntensity::Moderate,
            start_ordinal: 2,
            end_ordinal: 6,
            surface_oxide_colour: None,
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
            authorization: EffectAuthorization::Observation(ObservationPredicate::Forms),
            intensity: EffectIntensity::Moderate,
            start_ordinal: 2,
            end_ordinal: 6,
            surface_oxide_colour: None,
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

        assert_eq!(entry, 1_400, "a short drop must not become a slow glide");
        assert!(strong < moderate);
        assert!(moderate < subtle);
        assert_eq!(macroscopic_beat_duration_ms(None, false, true), 3_800);
    }
}
