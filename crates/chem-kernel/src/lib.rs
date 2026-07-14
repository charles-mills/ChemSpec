//! Trusted typed elaboration and deterministic structural expansion.
//!
//! Slice 4 resolves concise `.chems 1` source against an immutable catalogue
//! and external evidence packet, expands coefficients, atom mappings, and
//! reviewed operation templates, and produces typed HIR plus an inspectable
//! unexecuted certificate. Slice 5 owns graph execution and validation.

mod elaborate;
mod error;
mod evidence;
mod frames;
mod hir;
mod validate;

pub use elaborate::{expand_review_candidate, expand_trusted};
pub use error::{ExpansionError, ExpansionFailureClass};
pub use evidence::{
    EvidenceClaimRecord, EvidenceError, EvidencePacket, EvidencePacketReference,
    EvidencePacketSourceRecord, EvidencePredicate, ValidatedEvidencePacket,
};
pub use frames::{
    CurrentArtifactIdentity, FrameAtom, FrameAtomGroup, FrameChange, FrameCovalentEdge, FrameError,
    FrameFailureClass, FrameIonicAssociation, FrameMetallicDomain, FrameModelDisclosure,
    FrameObservation, FrameOperation, FrameTrace, ObservationStatus, SimulationFrame,
    SimulationFrames, generate_frames,
};
pub use hir::{
    CatalogueOrigin, CatalogueReference, CatalogueTrust, EvidenceOrigin, EvidenceTrust,
    ExpandedElectronContribution, ExpandedInstance, ExpandedIonicComponent, ExpandedOperation,
    ExpandedStructuralReaction, Provenance, ReactionSideKind, ResolvedApplicability,
    ResolvedEquationTerm, ResolvedEvidence, ResolvedModel, ResolvedObservation,
    ResolvedReactionClaim, ResolvedRuleApplication, ResolvedRuleBinding, ResolvedStructureBinding,
    SourceOrigin, SourceReference, TrustedExpandedStructuralReaction,
};
pub use validate::{
    DerivationTrust, KernelError, KernelFailureClass, ReviewCandidateStructuralDerivation,
    StructuralDerivation, StructuralLedger, StructuralState, ValidatedStructuralReaction,
    ValidationResult, validate_review_candidate, validate_trusted,
};
