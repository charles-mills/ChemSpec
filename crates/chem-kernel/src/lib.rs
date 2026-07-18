//! Trusted typed elaboration and deterministic structural expansion.
//!
//! Slice 4 resolves concise `.chems 1` source against an immutable catalogue
//! and external evidence packet, expands coefficients, atom mappings, and
//! reviewed operation templates, and produces typed HIR plus an inspectable
//! unexecuted certificate. Slice 5 owns graph execution and validation.

#[cfg(test)]
extern crate self as chem_kernel;

mod claim_consistency;
mod elaborate;
mod error;
mod evidence;
mod frames;
mod hir;
mod validate;

#[cfg(test)]
mod dative_tests;
#[cfg(test)]
mod slice5_tests;

pub use elaborate::{
    expand_proposed_declaration, expand_review_candidate, expand_reviewed_declaration,
    expand_trusted,
};
pub use error::{ExpansionError, ExpansionFailureClass};
pub use evidence::{
    EvidenceClaimRecord, EvidenceError, EvidencePacket, EvidencePacketReference,
    EvidencePacketSourceRecord, EvidencePredicate, ValidatedEvidencePacket,
};
pub use frames::{
    CurrentArtifactIdentity, FrameAtom, FrameAtomGroup, FrameChange, FrameCovalentEdge, FrameError,
    FrameFailureClass, FrameIonicAssociation, FrameMetallicDomain, FrameModelDisclosure,
    FrameObservation, FrameOperation, FrameTrace, ObservationStatus,
    ReviewCandidateFrameInspection, SimulationFrame, SimulationFrames, ValidatedDynamicFrames,
    generate_frames, inspect_review_candidate_frames,
};
pub use hir::{
    CatalogueOrigin, CatalogueReference, CatalogueTrust, EvidenceOrigin, EvidenceTrust,
    ExpandedElectronContribution, ExpandedInstance, ExpandedIonicComponent, ExpandedOperation,
    ExpandedStructuralReaction, Provenance, ReactionSideKind, ResolvedApplicability,
    ResolvedDeclarationBinding, ResolvedEquationTerm, ResolvedEvidence,
    ResolvedGeneralizedRuleApplication, ResolvedModel, ResolvedObservation, ResolvedReactionClaim,
    ResolvedRuleApplication, ResolvedRuleBinding, ResolvedStructureBinding, SourceOrigin,
    SourceReference, TrustedExpandedStructuralReaction,
};
pub use validate::{
    DerivationTrust, KernelError, KernelFailureClass, ReviewCandidateStructuralDerivation,
    StructuralDerivation, StructuralLedger, StructuralState, ValidatedStructuralReaction,
    ValidationResult, validate_review_candidate, validate_trusted,
};

pub(crate) use hir::{ExpandedStructuralReactionParts, ResolvedObservationCompatibility};
