//! Host extensions for custom components (SPEC §10.14; ROADMAP item 6).
//!
//! The std library is data-only and closed. A host that extends its `Library`
//! can draw the new component by registering a builder here. Extensions are
//! leaf nodes in phase 1: children stay owned by the renderer's normal slot
//! pipeline, and an extension that needs children should compose them from its
//! own model rather than the IR tree.

use crate::props::Props;
use genui_core::ir::{NodeStatus, Role};
use gpui_kit::{AnyElement, App, Window};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

/// The immutable node facts an extension receives.
#[derive(Clone, Debug)]
pub struct ExtensionNode {
    pub id: String,
    pub kind: String,
    pub role: Role,
    pub props: Props,
    pub status: NodeStatus,
}

pub type ExtensionBuilder = Rc<dyn Fn(&ExtensionNode, &mut Window, &mut App) -> AnyElement>;

/// Host-owned renderer registry. Cloned views share one registry.
#[derive(Clone, Default)]
pub struct ExtensionRegistry {
    builders: Rc<RefCell<BTreeMap<String, ExtensionBuilder>>>,
}

impl std::fmt::Debug for ExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionRegistry")
            .field("kinds", &self.kinds())
            .finish()
    }
}

impl ExtensionRegistry {
    pub fn register(
        &self,
        kind: impl Into<String>,
        builder: impl Fn(&ExtensionNode, &mut Window, &mut App) -> AnyElement + 'static,
    ) {
        self.builders
            .borrow_mut()
            .insert(kind.into(), Rc::new(builder));
    }

    pub fn get(&self, kind: &str) -> Option<ExtensionBuilder> {
        self.builders.borrow().get(kind).cloned()
    }

    pub fn kinds(&self) -> Vec<String> {
        self.builders.borrow().keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.builders.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.builders.borrow().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{IntoElement as _, ParentElement as _, div};

    #[test]
    fn extensions_are_registered_by_kind_and_shared_by_clone() {
        let registry = ExtensionRegistry::default();
        assert!(registry.is_empty());
        registry.register("Gauge", |_node, _, _| {
            div().child("gauge").into_any_element()
        });
        let clone = registry.clone();
        assert_eq!(clone.kinds(), vec!["Gauge".to_string()]);
        assert!(clone.get("Gauge").is_some());
        assert!(clone.get("Missing").is_none());
        assert_eq!(clone.len(), 1);
    }
}
