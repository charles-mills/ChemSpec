//! Renderer-independent explanatory simulation model: representative
//! particle identities, reaction stages, phase behaviour, playback state,
//! and conversion from `ValidatedExperiment` to `SimulationFrame`.
//!
//! Simulation state can never consume more of a species than the validated
//! result permits. The deterministic model (`U-103`) starts after the
//! `SimulationFrame` schema is frozen (`F-006`).
