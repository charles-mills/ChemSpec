//! Trusted typed elaboration and chemistry derivation boundary.

mod diagnostic;
mod elaborate;
mod hir;
mod procedure;
mod source;

pub use diagnostic::{ElaborationDiagnostic, ElaborationStatus};
pub use elaborate::{ElaborationResult, elaborate};
pub use hir::*;
pub use procedure::{ProcedureResult, execute_procedure};
