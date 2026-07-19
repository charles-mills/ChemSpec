//! Typed elaboration and authoritative deterministic structural validation.
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
    expand_proposed_declaration, expand_provisional, expand_reference, expand_reviewed_declaration,
};
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
    CatalogueOrigin, CatalogueProvenance, CatalogueReference, EvidenceOrigin, EvidenceProvenance,
    ExpandedElectronContribution, ExpandedInstance, ExpandedIonicComponent, ExpandedOperation,
    ExpandedStructuralReaction, Provenance, ReactionSideKind, ReferenceExpandedStructuralReaction,
    ResolvedApplicability, ResolvedDeclarationBinding, ResolvedEquationTerm, ResolvedEvidence,
    ResolvedGeneralizedRuleApplication, ResolvedModel, ResolvedObservation, ResolvedReactionClaim,
    ResolvedRuleApplication, ResolvedRuleBinding, ResolvedStructureBinding, SourceOrigin,
    SourceReference,
};
pub use validate::{
    DerivationProvenance, KernelError, KernelFailureClass, StructuralDerivation, StructuralLedger,
    StructuralState, ValidatedProvisionalStructuralReaction, ValidatedStructuralReaction,
    ValidationResult, validate_provisional, validate_reference,
};

pub(crate) use hir::{ExpandedStructuralReactionParts, ResolvedObservationCompatibility};
