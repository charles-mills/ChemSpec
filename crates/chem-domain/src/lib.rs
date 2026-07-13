//! Pure, stable domain types shared by every ChemSpec crate: formulas,
//! charges, phases, quantities, substances, reactions, assumptions,
//! derivations, and `ValidatedExperiment`.
//!
//! No parsing, networking, Iced, or GPU dependencies are allowed here.
//!
//! The shared contracts (`ParsedExperiment`, `ValidatedExperiment`,
//! `Diagnostic`, `Derivation`, ...) are defined by task `F-002`. Until that
//! lands, this crate only pins the workspace boundary (`F-001`).
