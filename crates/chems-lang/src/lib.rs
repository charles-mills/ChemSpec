//! Source-language concerns for `.chems`: lexer, parser, source spans,
//! syntax tree, formatter, syntax diagnostics, and serialization.
//!
//! This crate may use domain primitives such as formulas and quantities, but
//! it never decides whether a reaction is chemically supported — that is
//! `chem-engine` authority.
//!
//! Parser work (`L-101`) begins only after the `.chems` v0 grammar is frozen
//! at Gate 0 (`F-003`).
