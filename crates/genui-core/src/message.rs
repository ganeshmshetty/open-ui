//! Stored messages and events to the model (SPEC §§9.2–9.3).
use crate::{
    diag::{Code, Diagnostic},
    r#lib::LibrarySpec,
    value::Value,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Identity of the library that generated a prompt and validated a message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibRef {
    pub id: String,
    pub version: String,
    pub hash: String,
}

impl LibRef {
    pub fn of(spec: &LibrarySpec) -> Self {
        Self {
            id: spec.id.clone(),
            version: spec.version.clone(),
            hash: spec.hash.clone(),
        }
    }
}

/// Host-optional persistence of evaluated state (SPEC §§7.2,9.2). Values are plain data.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StateSnapshot {
    #[serde(default = "snapshot_version")]
    pub v: u32,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

fn snapshot_version() -> u32 {
    1
}

/// A display segment with the language major carried by its fence (SPEC §9.2).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum StoredSegment {
    Markdown { text: String },
    Program { text: String, major: u32 },
}

/// The model's reply as stored for history replay (SPEC §9.2). Programs are never IR.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StoredMessage {
    pub v: u32,
    pub segments: Vec<StoredSegment>,
    pub lib: LibRef,
    pub prompt_digest: String,
    pub complete: bool,
    pub fallback_text: String,
    pub diagnostics: Vec<Diagnostic>,
    pub state: Option<StateSnapshot>,
}

impl StoredMessage {
    /// Return the `lib.mismatch` info diagnostic when the recorded library identity
    /// differs from the current one. Replay still proceeds (SPEC §4.3).
    pub fn library_mismatch(&self, current: &LibrarySpec) -> Option<Diagnostic> {
        if self.lib.id == current.id
            && self.lib.version == current.version
            && self.lib.hash == current.hash
        {
            return None;
        }
        let mut diagnostic = Diagnostic::new(Code::LibMismatch, None, None);
        diagnostic.message = format!(
            "stored library {} {} {} differs from current {} {} {}",
            self.lib.id, self.lib.version, self.lib.hash, current.id, current.version, current.hash
        );
        diagnostic.expected = Some(format!(
            "{} {} {}",
            current.id, current.version, current.hash
        ));
        diagnostic.found = Some(format!(
            "{} {} {}",
            self.lib.id, self.lib.version, self.lib.hash
        ));
        Some(diagnostic)
    }

    /// The single stored program's source text, if the message contains one.
    pub fn program_text(&self) -> Option<String> {
        let mut text = String::new();
        for segment in &self.segments {
            if let StoredSegment::Program { text: program, .. } = segment {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(program);
            }
        }
        (!text.is_empty()).then_some(text)
    }
}

/// A UI interaction once the host lets it leave the process (SPEC §§7.8,9.3).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEvent {
    pub v: u32,
    #[serde(default = "event_source")]
    pub source: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stmt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

fn event_source() -> String {
    "ui_event".into()
}

impl ModelEvent {
    pub fn new(kind: &str, label: &str) -> Self {
        Self {
            v: 1,
            source: event_source(),
            kind: kind.into(),
            text: None,
            label: label.into(),
            ctx: None,
            node: None,
            stmt: None,
            msg: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Code;
    use crate::r#lib::{GroupDef, Library};

    fn stored() -> StoredMessage {
        StoredMessage {
            v: 1,
            segments: vec![
                StoredSegment::Markdown {
                    text: "Before.\n".into(),
                },
                StoredSegment::Program {
                    text: "root = Text(\"Hi\")\n".into(),
                    major: 1,
                },
            ],
            lib: LibRef {
                id: "std".into(),
                version: "0.1.0".into(),
                hash: "abc".into(),
            },
            prompt_digest: "0".repeat(32),
            complete: true,
            fallback_text: "Before.\nHi".into(),
            diagnostics: vec![Diagnostic::new(Code::NoRoot, None, None)],
            state: Some(StateSnapshot {
                v: 1,
                values: [("count".into(), Value::Int(3))].into(),
            }),
        }
    }

    #[test]
    fn stored_messages_round_trip_their_plain_json() {
        let message = stored();
        let text = serde_json::to_string(&message).unwrap();
        assert_eq!(
            serde_json::from_str::<StoredMessage>(&text).unwrap(),
            message
        );
        assert!(!text.contains("feedback"));
    }

    #[test]
    fn program_text_joins_only_program_segments() {
        let mut message = stored();
        assert_eq!(
            message.program_text().as_deref(),
            Some("root = Text(\"Hi\")\n")
        );
        message.segments.push(StoredSegment::Program {
            text: "root = Divider()\n".into(),
            major: 1,
        });
        assert_eq!(
            message.program_text().as_deref(),
            Some("root = Text(\"Hi\")\nroot = Divider()\n")
        );
    }

    #[test]
    fn identity_comparison_covers_id_version_and_hash() {
        let library = Library::builder("std", "0.1.0")
            .group(GroupDef::new("Test", 1, ""))
            .component(
                crate::r#lib::ComponentDef::new("Text", crate::ir::Role::Paragraph)
                    .group("Test")
                    .doc("text")
                    .req("text", crate::r#lib::T::text())
                    .to_text("{text}"),
            )
            .build()
            .unwrap();
        let message = stored();
        assert_eq!(
            message.library_mismatch(library.spec()).unwrap().code,
            Code::LibMismatch
        );
        let mut same = message.clone();
        same.lib.hash = library.hash().into();
        same.lib.version = library.spec().version.clone();
        assert!(same.library_mismatch(library.spec()).is_none());
    }
}
