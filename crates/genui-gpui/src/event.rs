//! Renderer-originated input (SPEC §§7.8, 10.1).
//!
//! The patch stream identifies nodes by their stable path string, while the
//! core's `UiEvent` uses arena `NodeId`s. The M8 renderer therefore emits this
//! path-addressed event and the host adapter resolves it with
//! `Session::activate_at` / `edit_at` / `blur_at`. M7's input work can adopt
//! direct `UiEvent` emission without changing the wire format.

use genui_core::{ir::NodeId, value::Value};

#[derive(Clone, Debug, PartialEq)]
pub enum RendererEvent {
    /// A button was activated. `prop` is the action argument to run
    /// (`on_click` for `Button`).
    Activate { path: String, prop: String },
    /// A bound value changed, for example a Tabs selection.
    Edit { path: String, value: Value },
    /// A field lost focus.
    Blur { path: String },
}

impl RendererEvent {
    pub fn path(&self) -> &str {
        match self {
            Self::Activate { path, .. } | Self::Edit { path, .. } | Self::Blur { path } => path,
        }
    }

    /// Resolves the path to a live node id. The host uses this once per event.
    pub fn node_id(&self, session: &genui_core::Session) -> Option<NodeId> {
        session.node_id_at(self.path())
    }
}
