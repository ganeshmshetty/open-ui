//! A reference host policy for the gallery, the sample chat host and the
//! transcript replay (SPEC §§10.7, 11; ROADMAP item 8).
//!
//! This module performs no I/O and opens no URLs. It decides what a host MAY
//! do with each request: read-only tools and refs in the allowlists are
//! delivered from bounded sample data; everything else is denied and logged.
//! The sample chat binary owns the file/terminal integration.

use crate::{event::RendererEvent, table::MAX_TABLE_ROWS};
use genui_core::{
    Session, SharedMap,
    session::{HostError, HostRequest, ReqKind, Update},
    validate::Options,
    value::Value,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The host's answer recorded in a transcript.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostDecision {
    Delivered,
    Denied,
    Recorded,
    DeniedUrl,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostTrace {
    pub request: u64,
    pub kind: String,
    pub decision: HostDecision,
    pub detail: String,
}

/// Allowlists and effect gates. The defaults are deliberately closed.
#[derive(Clone, Debug, Default)]
pub struct HostPolicy {
    pub tools: BTreeSet<String>,
    pub refs: BTreeSet<String>,
    pub url_hosts: BTreeSet<String>,
    /// Auto-approve read-only queries whose tool is allowlisted.
    pub approve_queries: bool,
    /// Let `open` requests through (the sample host still does not open).
    pub allow_open: bool,
    /// Let custom `emit` actions through to the transcript.
    pub allow_emit: bool,
}

impl HostPolicy {
    pub fn from_options(options: &Options) -> Self {
        Self {
            tools: options.tools.clone(),
            refs: options.refs.clone(),
            url_hosts: options.url_hosts.clone(),
            approve_queries: true,
            allow_open: false,
            allow_emit: true,
        }
    }

    /// The demo policy: the conformance host allowlists, read-only tools
    /// approved, `open` denied.
    pub fn demo() -> Self {
        Self {
            tools: BTreeSet::from(["sales.by_month".into(), "tickets.list".into()]),
            refs: BTreeSet::from(["t4.orders".into(), "t4.people".into(), "t4.places".into()]),
            url_hosts: BTreeSet::from(["example.com".into()]),
            approve_queries: true,
            allow_open: false,
            allow_emit: true,
        }
    }
}

/// Bounded sample values keyed by ref id or tool name.
#[derive(Clone, Debug, Default)]
pub struct HostData {
    pub refs: BTreeMap<String, Value>,
    pub queries: BTreeMap<String, Value>,
}

impl HostData {
    pub fn demo() -> Self {
        const REFS: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../conformance/libs/refs.json"
        ));
        const QUERIES: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../conformance/libs/queries.json"
        ));
        let refs = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(REFS)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(key, value)| Some((key, from_json(value)?)))
            .collect();
        let queries = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(QUERIES)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(key, value)| Some((key, from_json(value)?)))
            .collect();
        Self { refs, queries }
    }

    /// One ref value, capped to the table stress limit.
    pub fn ref_value(&self, id: &str) -> Option<Value> {
        cap_value(self.refs.get(id)?.clone())
    }

    pub fn query_value(&self, tool: &str) -> Option<Value> {
        cap_value(self.queries.get(tool)?.clone())
    }
}

fn from_json(value: serde_json::Value) -> Option<Value> {
    serde_json::from_value(value).ok()
}

fn cap_value(value: Value) -> Option<Value> {
    match value {
        Value::List(items) => Some(Value::List(std::sync::Arc::new(
            items.iter().take(MAX_TABLE_ROWS).cloned().collect(),
        ))),
        other => Some(other),
    }
}

/// The host loop state. It records every decision; the transcript writer
/// serializes `trace` and the sample host prints it.
#[derive(Clone, Debug)]
pub struct SampleHost {
    pub policy: HostPolicy,
    pub data: HostData,
    pub trace: Vec<HostTrace>,
    pub events: Vec<RendererEvent>,
}

impl SampleHost {
    pub fn new(policy: HostPolicy, data: HostData) -> Self {
        Self {
            policy,
            data,
            trace: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn demo() -> Self {
        Self::new(HostPolicy::demo(), HostData::demo())
    }

    /// Handles every request in `update`, including requests emitted by the
    /// resulting deliveries. Returns the deliveries' updates in order.
    pub fn settle(&mut self, session: &mut Session, update: &Update) -> Vec<Update> {
        let mut queue: VecDeque<HostRequest> = update.requests.iter().cloned().collect();
        let mut updates = Vec::new();
        let mut guard = 0;
        while let Some(request) = queue.pop_front() {
            guard += 1;
            if guard > 256 {
                self.trace.push(HostTrace {
                    request: request.id.0,
                    kind: "limit".into(),
                    decision: HostDecision::Denied,
                    detail: "host request loop guard".into(),
                });
                break;
            }
            if let Some(next) = self.respond(session, &request) {
                queue.extend(next.requests.iter().cloned());
                updates.push(next);
            }
        }
        updates
    }

    fn respond(&mut self, session: &mut Session, request: &HostRequest) -> Option<Update> {
        let (kind, decision, detail, result) = match &request.kind {
            ReqKind::Query {
                tool,
                needs_approval,
                ..
            } => {
                let allowed = self.policy.tools.contains(tool)
                    && self.policy.approve_queries
                    && (!needs_approval || self.policy.approve_queries);
                match (allowed, self.data.query_value(tool)) {
                    (true, Some(value)) => (
                        format!("query:{tool}"),
                        HostDecision::Delivered,
                        "allowlisted read-only tool".into(),
                        Some(Ok(value)),
                    ),
                    (true, None) => (
                        format!("query:{tool}"),
                        HostDecision::Denied,
                        "no sample data for tool".into(),
                        Some(Err(HostError::Denied)),
                    ),
                    (false, _) => (
                        format!("query:{tool}"),
                        HostDecision::Denied,
                        "tool not approved by host policy".into(),
                        Some(Err(HostError::Denied)),
                    ),
                }
            }
            ReqKind::ResolveRef { reference, .. } => {
                let allowed = self.policy.refs.contains(reference);
                match (allowed, self.data.ref_value(reference)) {
                    (true, Some(value)) => (
                        format!("ref:{reference}"),
                        HostDecision::Delivered,
                        "catalog ref from host data".into(),
                        Some(Ok(value)),
                    ),
                    _ => (
                        format!("ref:{reference}"),
                        HostDecision::Denied,
                        "ref not in the host catalog".into(),
                        Some(Err(HostError::Denied)),
                    ),
                }
            }
            ReqKind::OpenUrl { url, .. } => {
                let (allowed, detail) = self.check_url(url);
                (
                    "open_url".into(),
                    if allowed {
                        HostDecision::Recorded
                    } else {
                        HostDecision::DeniedUrl
                    },
                    detail,
                    None,
                )
            }
            ReqKind::Emit { name, .. } => (
                format!("emit:{name}"),
                if self.policy.allow_emit {
                    HostDecision::Recorded
                } else {
                    HostDecision::Denied
                },
                "custom event recorded for the transcript".into(),
                None,
            ),
            ReqKind::Say { label, .. } => (
                "say".into(),
                HostDecision::Recorded,
                format!("model event label {label:?}"),
                None,
            ),
        };
        self.trace.push(HostTrace {
            request: request.id.0,
            kind,
            decision,
            detail,
        });
        result.map(|result| session.deliver(request.id, result))
    }

    fn check_url(&self, url: &str) -> (bool, String) {
        if !self.policy.allow_open {
            return (false, "open is disabled by host policy".into());
        }
        let Some(rest) = url.strip_prefix("https://") else {
            return (false, "only https URLs are allowed".into());
        };
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        if self.policy.url_hosts.contains(host) {
            (true, format!("allowlisted host {host}"))
        } else {
            (false, format!("host {host:?} is not allowlisted"))
        }
    }

    /// Image fetches are host-owned (SPEC §10.3). The same URL allowlist
    /// applies before bytes enter the renderer.
    pub fn image_allowed(&self, src: &str) -> bool {
        let Some(rest) = src.strip_prefix("https://") else {
            return false;
        };
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        self.policy.url_hosts.contains(host)
    }

    /// Converts a path-addressed renderer event into the core entry point.
    pub fn event(&mut self, session: &mut Session, event: RendererEvent) -> Update {
        self.events.push(event.clone());
        match event {
            RendererEvent::Activate { path, prop } => session.activate_at(&path, &prop),
            RendererEvent::Edit { path, value } => session.edit_at(&path, value),
            RendererEvent::Blur { path } => session.blur_at(&path),
        }
    }

    pub fn trace_json(&self) -> Vec<serde_json::Value> {
        self.trace
            .iter()
            .filter_map(|trace| serde_json::to_value(trace).ok())
            .collect()
    }

    /// A compact policy description for transcript metadata.
    pub fn policy_summary(&self) -> SharedMap<String, String> {
        let mut summary = SharedMap::new();
        summary.insert(
            "tools".into(),
            self.policy.tools.iter().cloned().collect::<Vec<_>>().join(","),
        );
        summary.insert(
            "refs".into(),
            self.policy.refs.iter().cloned().collect::<Vec<_>>().join(","),
        );
        summary.insert(
            "open".into(),
            if self.policy.allow_open { "allow" } else { "deny" }.into(),
        );
        summary.insert(
            "approve_queries".into(),
            self.policy.approve_queries.to_string(),
        );
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genui_core::{SessionOptions, session::InputMode};
    use std::sync::Arc;

    fn session() -> Session {
        let library = genui_std::std_library().expect("std");
        let options = Options {
            tools: ["sales.by_month".into()].into(),
            refs: ["t4.orders".into()].into(),
            url_hosts: ["example.com".into()].into(),
            ..Options::default()
        };
        let mut session = Session::new(SessionOptions {
            library: Arc::new(library),
            validation: options,
            input: InputMode::Program,
            prompt_digest: String::new(),
        });
        session.push(
            b"root=Table([TableColumn(\"Order\",\"id\")],ref(\"t4.orders\"))",
        );
        session
    }

    #[test]
    fn demo_data_parses_plain_json_values() {
        let data = HostData::demo();
        assert_eq!(data.refs.len(), 3);
        assert_eq!(data.queries.len(), 2);
        assert_eq!(data.ref_value("t4.orders").map(|value| match value {
            Value::List(items) => items.len(),
            _ => 0,
        }), Some(5));
    }

    #[test]
    fn allowlisted_refs_are_delivered_and_recorded() {
        let mut session = session();
        let mut host = SampleHost::demo();
        let update = session.finish();
        let updates = host.settle(&mut session, &update);
        assert!(!updates.is_empty());
        assert!(host.trace.iter().any(|trace| trace.decision == HostDecision::Delivered));
        assert_eq!(host.data.ref_value("t4.orders").map(|v| match v {
            Value::List(items) => items.len(),
            _ => 0,
        }), Some(5));
    }

    #[test]
    fn an_unknown_ref_is_denied_not_invented() {
        let mut session = session();
        let mut host = SampleHost::demo();
        host.policy.refs.clear();
        let update = session.finish();
        host.settle(&mut session, &update);
        assert!(host.trace.iter().any(|trace| trace.decision == HostDecision::Denied));
    }

    #[test]
    fn open_urls_require_https_and_an_allowlisted_host() {
        let host = SampleHost::demo();
        assert!(!host.check_url("http://example.com/x").0);
        assert!(!host.check_url("https://evil.example/x").0);
        assert!(!host.check_url("https://example.com/x").0, "open is off by default");
        let mut host = host;
        host.policy.allow_open = true;
        assert!(host.check_url("https://example.com/x").0);
    }

    #[test]
    fn image_fetches_require_https_and_the_host_allowlist() {
        let host = SampleHost::demo();
        assert!(host.image_allowed("https://example.com/photo.png"));
        assert!(!host.image_allowed("http://example.com/photo.png"));
        assert!(!host.image_allowed("https://tracker.example/photo.png"));
        assert!(!host.image_allowed("file:///etc/passwd"));
    }

    #[test]
    fn every_trace_entry_is_serializable() {
        let mut session = session();
        let mut host = SampleHost::demo();
        let update = session.finish();
        host.settle(&mut session, &update);
        let json = host.trace_json();
        assert!(!json.is_empty());
        assert!(json[0].get("decision").is_some());
    }
}
