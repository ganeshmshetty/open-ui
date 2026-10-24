//! Renderer capabilities (SPEC §10.2).
//!
//! The same flags drive `genui-cli`/`Session` validation and the prompt's
//! fallbacks. M7 renders text input, charts and markdown, shows image
//! placeholders until the M8 loader, and keeps code highlighting off.

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capabilities {
    pub charts: bool,
    pub images: bool,
    pub markdown: bool,
    pub code_highlight: bool,
    pub text_input: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::m7()
    }
}

impl Capabilities {
    /// What the M7 renderer can actually draw.
    pub const fn m7() -> Self {
        Self {
            charts: true,
            images: true,
            markdown: true,
            code_highlight: false,
            text_input: true,
        }
    }

    /// The M6 capability set, kept for comparisons and historical tests.
    pub const fn m6() -> Self {
        Self {
            text_input: false,
            ..Self::m7()
        }
    }

    /// The core's capability names (SPEC §8.8), for `SessionOptions`.
    pub fn names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        if self.charts {
            names.insert("charts".into());
        }
        if self.images {
            names.insert("images".into());
        }
        if self.markdown {
            names.insert("markdown".into());
        }
        if self.code_highlight {
            names.insert("code_highlight".into());
        }
        if self.text_input {
            names.insert("text_input".into());
        }
        names
    }

    /// Capabilities a core validation `Options` should use for this renderer.
    pub fn validation_options(&self) -> genui_core::validate::Options {
        genui_core::validate::Options {
            capabilities: self.names(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m7_capabilities_are_explicit_and_match_the_validation_names() {
        let caps = Capabilities::m7();
        assert!(caps.charts);
        assert!(caps.images);
        assert!(caps.markdown);
        assert!(!caps.code_highlight);
        assert!(caps.text_input);
        assert_eq!(
            caps.names(),
            BTreeSet::from([
                "charts".to_string(),
                "images".into(),
                "markdown".into(),
                "text_input".into(),
            ])
        );
        let options = caps.validation_options();
        assert!(options.capabilities.contains("text_input"));
        assert!(!Capabilities::m6().text_input);
    }
}
