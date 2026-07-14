//! Trusted typed elaboration and chemistry derivation boundary.

mod diagnostic;
mod elaborate;
mod hir;
mod source;

pub use diagnostic::{ElaborationDiagnostic, ElaborationStatus};
pub use elaborate::{ElaborationResult, elaborate};
pub use hir::*;
