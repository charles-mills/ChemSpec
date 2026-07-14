#![forbid(unsafe_code)]

//! Deterministic, renderer-independent planning over trusted kernel frames.
//!
//! This crate owns pacing and macroscopic scene composition. It never parses
//! `.chems`, resolves rules, or constructs chemistry.

use std::fmt;

use chem_catalogue::ObservationPredicate;
use chem_domain::{AtomGroupId, AtomId, ContentDigest, StructuralOperationView};
use chem_kernel::{ObservationStatus, SimulationFrame, SimulationFrames};

pub const VIRTUAL_ONLY_DISCLOSURE: &str = "Virtual educational model—not a laboratory procedure. Timing, scale, motion, and camera movement are illustrative.";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EducationalScene {
    pub kind: EducationalSceneKind,
    pub start_frame: ContentDigest,
    pub end_frame: ContentDigest,
    pub duration_ms: u32,
    pub explanation: Option<ExplanationLabel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EducationalPlan {
    pub id: ContentDigest,
    pub scenes: Vec<EducationalScene>,
}

/// Adds explanatory pacing around the immutable operation sequence without
/// changing or reordering any chemical state.
///
/// # Errors
///
/// Returns an error if frames are absent, non-contiguous, missing their
/// validated operation, or cannot provide their bound generation digest.
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
            1_600,
            None,
        ),
        scene(
            EducationalSceneKind::ReactantSetup,
            first,
            first,
            1_800,
            None,
        ),
        scene(EducationalSceneKind::Equation, first, first, 1_800, None),
    ];
    for pair in sequence.windows(2) {
        let before = &pair[0];
        let after = &pair[1];
        let operation = after
            .active_operation()
            .ok_or(PlanError::MissingOperation(after.ordinal()))?;
        let label = operation_label(operation.operation.view());
        scenes.push(scene(
            EducationalSceneKind::StructuralChange,
            before,
            after,
            1_300,
            None,
        ));
        scenes.push(scene(
            EducationalSceneKind::ExplanationPause,
            after,
            after,
            explanation_duration(&label.text),
            Some(label),
        ));
        for observation in after
            .observations()
            .iter()
            .filter(|observation| observation.status == ObservationStatus::Active)
        {
            let text = observation_text(observation.predicate, observation.value.as_deref());
            scenes.push(scene(
                EducationalSceneKind::ObservationConnection,
                after,
                after,
                explanation_duration(&text),
                Some(ExplanationLabel {
                    kind: ExplanationLabelKind::ObservationExplanation,
                    text,
                    target_atoms: after
                        .product_membership()
                        .values()
                        .flatten()
                        .map(|atom| atom.as_str().to_owned())
                        .collect(),
                    connector: true,
                }),
            ));
        }
    }
    scenes.push(scene(
        EducationalSceneKind::Summary,
        last,
        last,
        2_200,
        None,
    ));
    Ok(EducationalPlan {
        id: frames.digest().map_err(|_| PlanError::Digest)?,
        scenes,
    })
}

fn scene(
    kind: EducationalSceneKind,
    start: &SimulationFrame,
    end: &SimulationFrame,
    duration_ms: u32,
    explanation: Option<ExplanationLabel>,
) -> EducationalScene {
    EducationalScene {
        kind,
        start_frame: start.trace().state_digest,
        end_frame: end.trace().state_digest,
        duration_ms,
        explanation,
    }
}

fn operation_label(operation: StructuralOperationView<'_>) -> ExplanationLabel {
    let (text, atoms) = match operation {
        StructuralOperationView::CleaveCovalent { left, right, .. } => (
            "This shared covalent relationship has been cleaved.",
            vec![left.as_str(), right.as_str()],
        ),
        StructuralOperationView::FormCovalent { left, right, .. } => (
            "A new shared electron pair forms this covalent bond.",
            vec![left.as_str(), right.as_str()],
        ),
        StructuralOperationView::CleaveDative {
            donor, acceptor, ..
        } => (
            "This coordinate bond is cleaved while its electron origin remains explicit.",
            vec![donor.as_str(), acceptor.as_str()],
        ),
        StructuralOperationView::FormDative {
            donor, acceptor, ..
        } => (
            "The donor supplies both electrons to this coordinate bond.",
            vec![donor.as_str(), acceptor.as_str()],
        ),
        StructuralOperationView::ChangeCovalent { left, right, .. } => (
            "The validated covalent bond order changes.",
            vec![left.as_str(), right.as_str()],
        ),
        StructuralOperationView::AssociateIonic { association } => (
            "Oppositely charged components are now associated.",
            association
                .components()
                .iter()
                .map(AtomGroupId::as_str)
                .collect(),
        ),
        StructuralOperationView::DissociateIonic { .. } => {
            ("The ionic association separates.", Vec::new())
        }
        StructuralOperationView::ReleaseMetallic { site, .. } => (
            "A site leaves the metallic electron domain.",
            vec![site.as_str()],
        ),
        StructuralOperationView::JoinMetallic { site, .. } => (
            "A site joins the metallic electron domain.",
            vec![site.as_str()],
        ),
        StructuralOperationView::TransferElectron {
            donor, acceptor, ..
        } => (
            "An electron transfers from the donor to the accepting atom.",
            vec![donor.as_str(), acceptor.as_str()],
        ),
        StructuralOperationView::AssignProduct { atoms, .. } => (
            "These conserved atoms are now assigned to a validated product.",
            atoms.iter().map(AtomId::as_str).collect(),
        ),
    };
    ExplanationLabel {
        kind: ExplanationLabelKind::StructuralChangeExplanation,
        text: text.to_owned(),
        target_atoms: atoms.into_iter().map(str::to_owned).collect(),
        connector: true,
    }
}

fn observation_text(predicate: ObservationPredicate, value: Option<&str>) -> String {
    match predicate {
        ObservationPredicate::Evolves => "Gas evolution is now observable.".to_owned(),
        ObservationPredicate::Disappears => "The reactant is visibly consumed.".to_owned(),
        ObservationPredicate::Forms => "The validated product is now established.".to_owned(),
        ObservationPredicate::Colour => value.map_or_else(
            || "A validated colour observation is now active.".to_owned(),
            |colour| format!("The observed colour is {colour}."),
        ),
    }
}

fn explanation_duration(text: &str) -> u32 {
    let words = u32::try_from(text.split_whitespace().count()).unwrap_or(u32::MAX);
    words
        .saturating_mul(210)
        .saturating_add(1_200)
        .clamp(2_200, 5_500)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub disclosure: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenePlan {
    pub reaction: ContentDigest,
    pub environment: AssetProfile,
    pub objects: Vec<PresentationObject>,
    pub effects: Vec<PresentationEffect>,
    pub camera: Vec<CameraCue>,
    pub disclosure: String,
}

/// Binds a host-selected visual profile to a trusted generation. Effects whose
/// observation trigger is absent are rejected instead of guessed.
///
/// # Errors
///
/// Returns an error when an effect lacks a matching validated observation or
/// the trusted frame digest is unavailable.
pub fn compile_real_world_plan(
    frames: &SimulationFrames,
    profile: &PresentationProfile,
) -> Result<ScenePlan, PlanError> {
    let predicates = frames
        .frames()
        .iter()
        .flat_map(SimulationFrame::observations)
        .map(|observation| observation.predicate)
        .collect::<Vec<_>>();
    if profile
        .effects
        .iter()
        .any(|effect| !predicates.contains(&effect.trigger))
    {
        return Err(PlanError::UnsupportedEffectTrigger);
    }
    Ok(ScenePlan {
        reaction: frames.digest().map_err(|_| PlanError::Digest)?,
        environment: profile.environment,
        objects: profile.objects.clone(),
        effects: profile.effects.clone(),
        camera: profile.camera.clone(),
        disclosure: profile.disclosure.clone(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    MissingFrames,
    InvalidFrameSequence,
    MissingOperation(u32),
    UnsupportedEffectTrigger,
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
            Self::UnsupportedEffectTrigger => {
                formatter.write_str("presentation effect has no validated observation trigger")
            }
            Self::Digest => formatter.write_str("trusted frame digest is unavailable"),
        }
    }
}

impl std::error::Error for PlanError {}
