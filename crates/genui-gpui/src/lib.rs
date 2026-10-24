//! GPUI renderer for the generative UI DSL (SPEC §10).
//!
//! M6 provides the pinned GPUI integration, `UiView`'s retained tree and batch
//! application, the layout/text content components, theming from
//! `spec/DESIGN.md`, the gallery/state-sheet binary and the streaming dashboard
//! demo. Inputs and forms are M7; data tables, lists and tabs are M8.
//!
//! The crate does no I/O and owns no networking, tool logic or model access.

#![forbid(unsafe_code)]

pub mod capabilities;
pub mod chart;
pub mod components;
pub mod data;
pub mod demo;
pub mod event;
pub mod examples;
pub mod format;
pub mod host;
pub mod image;
pub mod inspector;
pub mod layout;
pub mod metrics;
pub mod motion;
pub mod panics;
pub mod props;
pub mod state_sheet;
pub mod table;
pub mod theme;
pub mod tree;
pub mod view;

pub use capabilities::Capabilities;
pub use event::RendererEvent;
pub use image::{ImageError, ImageLimits, ImageState};
pub use theme::{ThemeMode, UiTheme};
pub use tree::{ApplyReport, NodeEntry, RetainedTree};
pub use view::UiView;
