//! UI-free, synchronous generative-UI core (SPEC §1.2).
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

pub mod diag;
pub mod syntax;

/// Validated intermediate representation (M2).
pub mod ir;
/// Synchronous evaluator and host interaction (M4).
pub mod runtime {}
/// Renderer-independent patch protocol (M3).
pub mod patch {}
/// Component library model and builder (M2).
#[path = "library.rs"]
pub mod r#lib;
/// Host-configurable evaluation and size limits (M2).
pub mod limits;
pub mod validate;
pub mod value;
