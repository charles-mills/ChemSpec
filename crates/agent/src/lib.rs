//! Provider-neutral agent orchestration: Codex CLI and Responses API
//! providers, preflight checks, workflow events, structured research
//! results, provenance, and the bounded repair protocol.
//!
//! This crate returns `.chems` source and provenance — never trusted
//! chemistry. Provider work (`A-101`+) starts after `ResearchResult`,
//! `EvidenceClaim`, and `AgentEvent` are frozen (`F-005`).
