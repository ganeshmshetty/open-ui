//! Renderer capabilities (SPEC §10.2).
//!
//! The same flags drive `genui-cli`/`Session` validation and the prompt's
//! fallbacks. M6 renders charts and markdown, shows image placeholders until
//! the M8 loader, keeps code highlighting off, and leaves text input to M7.

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
        Self::m6()
    }
}

impl Capabilities {
    /// What the M6 renderer can actually draw.
    pub const fn m6() -> Self {
        Self {
            charts: true,
            images: true,
            markdown: true,
            code_highlight: false,
            text_input: false,
        }
    }

    /// M8 keeps the same flags: data components are always available, images
    /// are host-delivered and text inputs remain M7.
    pub const fn m8() -> Self {
        Self::m6()
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
    fn m6_capabilities_are_explicit_and_match_the_validation_names() {
        let caps = Capabilities::m6();
        assert!(caps.charts);
        assert!(caps.images);
        assert!(caps.markdown);
        assert!(!caps.code_highlight);
        assert!(!caps.text_input);
        assert_eq!(
            caps.names(),
            BTreeSet::from(["charts".to_string(), "images".into(), "markdown".into()])
        );
        let options = caps.validation_options();
        assert!(!options.capabilities.contains("text_input"));
    }
}
