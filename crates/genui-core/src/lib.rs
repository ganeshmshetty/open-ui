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

/// Deterministic CommonMark fallback derived from IR (M5).
pub mod fallback;
/// Model-facing diagnostic feedback (M5).
pub mod feedback;
/// Validated intermediate representation (M2).
pub mod ir;
/// Stored messages, library identity and events to the model (M5).
pub mod message;
/// Host-facing tool and ref catalog types (M5).
pub mod protocol;
/// Synchronous evaluator and host interaction (M4).
pub mod runtime {}
pub use message::ModelEvent;
/// Component library model and builder (M2).
#[path = "library.rs"]
pub mod r#lib;
/// Host-configurable evaluation and size limits (M2).
pub mod limits;
/// Renderer-independent patch protocol (M3).
pub mod patch;
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
