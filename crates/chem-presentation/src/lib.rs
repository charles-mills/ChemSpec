#![forbid(unsafe_code)]

//! Deterministic, renderer-independent planning over trusted kernel frames.
//!
//! This crate owns pacing and macroscopic scene composition. It never parses
//! `.chems`, resolves rules, or constructs chemistry.

use std::collections::BTreeSet;
use std::fmt;

use chem_catalogue::ObservationPredicate;
use chem_domain::{AtomId, ContentDigest, IonicAssociationId, StructuralOperationView};
use chem_kernel::{ObservationStatus, SimulationFrame, SimulationFrames};

pub const VIRTUAL_ONLY_DISCLOSURE: &str = "Virtual educational model—not a laboratory procedure. Timing, scale, motion, and camera movement are illustrative.";

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
        } => (
            format!(
                "{} → {} · {count} {}",
                atom_symbol(before, after, donor),
                atom_symbol(before, after, acceptor),
                plural(count.into(), "electron")
            ),
            if count == 1 {
                "An electron jumps from one atom to the other: the giver becomes more positive and the receiver more negative."
                    .to_owned()
            } else {
                "Electrons jump from one atom to the other: the giver becomes more positive and the receiver more negative."
                    .to_owned()
            },
            atom_targets([donor, acceptor]),
        ),
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
}

/// A trusted observation that must activate before an object may be shown.
/// An expected value closes the binding over value-bearing predicates such as
/// precipitate colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectObservationBinding {
    pub predicate: ObservationPredicate,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectProfile {
    BubbleEmitter,
    GasRelease,
    SurfaceDisturbance,
    ObjectShrinkage,
    PrecipitateFormation,
    Clouding,
    ColourTransition,
    SplashEmitter,
    HeatDistortion,
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
                let duration_ms = match intensity {
                    Some(EffectIntensity::Strong) => 7_200,
                    Some(EffectIntensity::Moderate) => 6_400,
                    Some(EffectIntensity::Subtle) => 5_600,
                    None if start_ordinal == 0 => 4_200,
                    None => 4_400,
                };
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
                    duration_ms: if end_ordinal == final_ordinal {
                        duration_ms.max(5_600)
                    } else {
                        duration_ms
                    },
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
    UnsupportedEffectTrigger,
    UnsupportedObjectObservation,
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
            Self::UnsupportedEffectTrigger => formatter.write_str(
                "presentation effect precedes or lacks an active validated observation trigger",
            ),
            Self::UnsupportedObjectObservation => formatter.write_str(
                "presentation object precedes or mismatches its active validated observation",
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
    use super::{EducationalPlan, EducationalScene, EducationalSceneKind, TimelinePosition};

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
}
