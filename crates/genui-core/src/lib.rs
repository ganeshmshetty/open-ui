//! UI-free, synchronous generative-UI core (SPEC §1.2).
#![forbid(unsafe_code)]

pub mod diag;
pub mod syntax;

/// Validated intermediate representation (M2).
pub mod ir {}
/// Synchronous evaluator and host interaction (M4).
pub mod runtime {}
/// Renderer-independent patch protocol (M3).
pub mod patch {}
/// Host-configurable evaluation and size limits (M2).
pub mod limits;
/// Component library model and builder (M2).
pub mod r#lib {}
