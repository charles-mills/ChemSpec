//! The trusted deterministic kernel: versioned catalogue, reaction rules,
//! equation validation, stoichiometry, and validation derivations.
//!
//! This is the **only** crate allowed to construct a `ValidatedExperiment`.
//! The agent may propose chemistry; it cannot declare its own output valid.
//!
//! Catalogue loading (`C-101`) and precipitation validation (`C-102`) begin
//! after the canonical fixture is hand-reviewed (`F-004`).
