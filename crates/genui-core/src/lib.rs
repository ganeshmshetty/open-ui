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

pub mod canonical;
pub mod diag;
pub mod syntax;

/// Validated intermediate representation (M2).
pub mod ir;
/// Component library model and builder (M2).
#[path = "library.rs"]
pub mod r#lib;
/// Host-configurable evaluation and size limits (M2).
pub mod limits;
/// Renderer-independent patch protocol (M3).
pub mod patch;
/// Synchronous evaluator and explicit state-writing action entry point (M4).
pub mod runtime;
pub mod validate;
pub mod value;

mod maintenance;
pub mod session;
pub use maintenance::MaintenanceWork;
mod persistent;
pub use persistent::{SharedMap, SharedVector};
pub use session::{Session, SessionOptions, Update};

mod diagnostics;
pub use diagnostics::Diagnostics;

mod arena;
pub use arena::{ActionArena, ExpressionArena};
