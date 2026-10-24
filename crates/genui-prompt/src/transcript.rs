//! Recorded, deterministic conversations (SPEC §12.7; ROADMAP item 17).
//!
//! A transcript is host data, not model evidence: every file carries
//! `"label": "scripted-fixture"` and the recorder never claims a live model.
//! Replay runs the exact chunks, events, preloaded refs and query results
//! through a fresh `Session` and compares the canonical patch stream, state,
//! model events and diagnostics.

use genui_core::{
    Session, SessionOptions,
    diag::Severity,
    message::{ModelEvent, StateSnapshot},
    patch::PatchBatch,
    r#lib::Library,
    session::{HostError, InputMode, SessionPhase, Update},
    validate::Options,
    value::Value,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

pub const TRANSCRIPT_VERSION: u32 = 1;
pub const SCRIPTED_LABEL: &str = "scripted-fixture";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Transcript {
    pub v: u32,
    pub id: String,
    /// Always `scripted-fixture`; real-model evidence needs a different label
    /// and a recorded run (SPEC §12.7).
    pub label: String,
    pub prompt: String,
    pub prompt_digest: String,
    pub source: Option<String>,
    pub chunks: Vec<TranscriptChunk>,
    #[serde(default)]
    pub events: Vec<TranscriptEventAt>,
    #[serde(default)]
    pub preload: BTreeMap<String, Value>,
    #[serde(default)]
    pub query_results: BTreeMap<String, Value>,
    #[serde(default)]
    pub expect: Option<Expectation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptChunk {
    /// Milliseconds since the previous chunk; scripted, not measured model
    /// latency.
    pub at_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TranscriptEventAt {
    Activate {
        after_chunk: usize,
        path: String,
        prop: String,
    },
    Edit {
        after_chunk: usize,
        path: String,
        value: Value,
    },
    Blur {
        after_chunk: usize,
        path: String,
    },
    Advance {
        after_chunk: usize,
        ms: u64,
    },
    Reset {
        after_chunk: usize,
    },
}

impl TranscriptEventAt {
    fn after(&self) -> usize {
        match self {
            Self::Activate { after_chunk, .. }
            | Self::Edit { after_chunk, .. }
            | Self::Blur { after_chunk, .. }
            | Self::Advance { after_chunk, .. }
            | Self::Reset { after_chunk, .. } => *after_chunk,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    /// Canonical JSON for each applied batch, in order.
    pub patches: Vec<String>,
    pub state: StateSnapshot,
    /// Canonical JSON for each model event, in order.
    pub events: Vec<String>,
    /// `code: severity` strings, in order.
    pub diagnostics: Vec<String>,
    pub phase: String,
    pub requests: Vec<String>,
}

/// The deterministic result of running a transcript.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recording {
    pub patches: Vec<PatchBatch>,
    pub state: StateSnapshot,
    pub events: Vec<ModelEvent>,
    pub diagnostics: Vec<String>,
    pub phase: String,
    pub requests: Vec<String>,
}

impl Recording {
    pub fn expectation(&self) -> Result<Expectation, String> {
        Ok(Expectation {
            patches: self
                .patches
                .iter()
                .map(|batch| batch.canonical_json().map_err(|error| error.to_string()))
                .collect::<Result<Vec<_>, _>>()?,
            state: self.state.clone(),
            events: self
                .events
                .iter()
                .map(|event| {
                    genui_core::canonical::serialize(event).map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?,
            diagnostics: self.diagnostics.clone(),
            phase: self.phase.clone(),
            requests: self.requests.clone(),
        })
    }

    pub fn verify(&self, expected: &Expectation) -> Result<(), String> {
        let actual = self.expectation()?;
        if actual.patches != expected.patches {
            let index = actual
                .patches
                .iter()
                .zip(&expected.patches)
                .position(|(a, b)| a != b)
                .unwrap_or(actual.patches.len().min(expected.patches.len()));
            return Err(format!(
                "patch stream differs at batch {index}: got {:?}, want {:?}",
                actual.patches.get(index),
                expected.patches.get(index)
            ));
        }
        if actual.state != expected.state {
            return Err(format!(
                "state differs: got {:?}, want {:?}",
                actual.state, expected.state
            ));
        }
        if actual.events != expected.events {
            return Err(format!(
                "events differ: got {:?}, want {:?}",
                actual.events, expected.events
            ));
        }
        if actual.diagnostics != expected.diagnostics {
            return Err(format!(
                "diagnostics differ: got {:?}, want {:?}",
                actual.diagnostics, expected.diagnostics
            ));
        }
        if actual.phase != expected.phase {
            return Err(format!(
                "phase differs: got {:?}, want {:?}",
                actual.phase, expected.phase
            ));
        }
        if actual.requests != expected.requests {
            return Err(format!(
                "request trace differs: got {:?}, want {:?}",
                actual.requests, expected.requests
            ));
        }
        Ok(())
    }
}

/// Runs a transcript with no model, network or filesystem access.
pub fn record(
    transcript: &Transcript,
    library: Arc<Library>,
    options: &Options,
) -> Result<Recording, String> {
    if transcript.v != TRANSCRIPT_VERSION {
        return Err(format!("transcript version {} is unsupported", transcript.v));
    }
    let mut session = Session::new(SessionOptions {
        library,
        validation: options.clone(),
        input: InputMode::Message,
        prompt_digest: transcript.prompt_digest.clone(),
    });
    for (reference, value) in &transcript.preload {
        if !session.preload_ref(reference, value.clone()) {
            return Err(format!("preload for ref {reference:?} was rejected"));
        }
    }
    let mut recording = Recording::default();
    let mut events = transcript.events.clone();
    events.sort_by_key(TranscriptEventAt::after);
    let mut next_event = 0usize;
    for (index, chunk) in transcript.chunks.iter().enumerate() {
        let update = session.push(chunk.text.as_bytes());
        absorb(&mut recording, &update)?;
        while next_event < events.len() && events[next_event].after() <= index {
            run_event(&mut session, &events[next_event], &transcript.query_results, &mut recording)?;
            next_event += 1;
        }
    }
    let finish = session.finish();
    absorb(&mut recording, &finish)?;
    settle(&mut session, &finish.requests, &transcript.query_results, &mut recording)?;
    while next_event < events.len() {
        run_event(
            &mut session,
            &events[next_event],
            &transcript.query_results,
            &mut recording,
        )?;
        next_event += 1;
    }
    recording.state = session.state_snapshot();
    recording.phase = phase_name(session.phase()).into();
    recording.diagnostics = session
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.severity >= Severity::Info)
        .map(|diagnostic| format!("{:?}:{:?}", diagnostic.code, diagnostic.severity))
        .collect();
    Ok(recording)
}

fn absorb(recording: &mut Recording, update: &Update) -> Result<(), String> {
    if let Some(batch) = &update.patches {
        recording.patches.push(batch.clone());
    }
    recording.events.extend(update.model_events.iter().cloned());
    for request in &update.requests {
        recording.requests.push(request_label(request));
    }
    Ok(())
}

fn run_event(
    session: &mut Session,
    event: &TranscriptEventAt,
    queries: &BTreeMap<String, Value>,
    recording: &mut Recording,
) -> Result<(), String> {
    let update = event_update(session, event);
    if std::env::var_os("GENUI_TRANSCRIPT_DEBUG").is_some() {
        eprintln!("event {:?} -> requests {:?} events {:?}", event, update.requests, update.model_events);
    }
    absorb(recording, &update)?;
    for update in settle_updates(session, &update.requests, queries)? {
        absorb(recording, &update)?;
    }
    Ok(())
}

fn event_update(session: &mut Session, event: &TranscriptEventAt) -> Update {
    match event {
        TranscriptEventAt::Activate { path, prop, .. } => session.activate_at(path, prop),
        TranscriptEventAt::Edit { path, value, .. } => session.edit_at(path, value.clone()),
        TranscriptEventAt::Blur { path, .. } => session.blur_at(path),
        TranscriptEventAt::Advance { ms, .. } => session.advance_clock(*ms),
        TranscriptEventAt::Reset { .. } => session.reset(),
    }
}

/// Delivers scripted host results until no request remains. Returned updates
/// are in application order.
fn settle_updates(
    session: &mut Session,
    requests: &[genui_core::session::HostRequest],
    queries: &BTreeMap<String, Value>,
) -> Result<Vec<Update>, String> {
    let mut updates = Vec::new();
    let mut queue: std::collections::VecDeque<_> = requests.iter().cloned().collect();
    let mut guard = 0usize;
    while let Some(request) = queue.pop_front() {
        guard += 1;
        if guard > 256 {
            return Err("host request loop guard".into());
        }
        let result = match &request.kind {
            genui_core::session::ReqKind::ResolveRef { .. } => Some(Err(HostError::Denied)),
            genui_core::session::ReqKind::Query { tool, .. } => match queries.get(tool) {
                Some(value) => Some(Ok(value.clone())),
                None => Some(Err(HostError::Denied)),
            },
            genui_core::session::ReqKind::Say { .. }
            | genui_core::session::ReqKind::Emit { .. }
            | genui_core::session::ReqKind::OpenUrl { .. } => None,
        };
        if let Some(result) = result {
            let update = session.deliver(request.id, result);
            queue.extend(update.requests.iter().cloned());
            updates.push(update);
        }
    }
    Ok(updates)
}

fn settle(
    session: &mut Session,
    requests: &[genui_core::session::HostRequest],
    queries: &BTreeMap<String, Value>,
    recording: &mut Recording,
) -> Result<(), String> {
    for update in settle_updates(session, requests, queries)? {
        absorb(recording, &update)?;
    }
    Ok(())
}

fn request_label(request: &genui_core::session::HostRequest) -> String {
    match &request.kind {
        genui_core::session::ReqKind::ResolveRef { reference, .. } => format!("ref:{reference}"),
        genui_core::session::ReqKind::Query { tool, .. } => format!("query:{tool}"),
        genui_core::session::ReqKind::Say { .. } => "say".into(),
        genui_core::session::ReqKind::Emit { name, .. } => format!("emit:{name}"),
        genui_core::session::ReqKind::OpenUrl { .. } => "open_url".into(),
    }
}

/// An incremental transcript runner used by the sample host. Each `step`
/// returns the updates for one chunk (or the finish) in application order,
/// including scripted events and host deliveries.
pub struct LiveRunner {
    session: Session,
    transcript: Transcript,
    queries: BTreeMap<String, Value>,
    events: Vec<TranscriptEventAt>,
    chunk: usize,
    event: usize,
    finished: bool,
}

impl LiveRunner {
    pub fn new(
        transcript: Transcript,
        library: Arc<Library>,
        options: &Options,
    ) -> Result<Self, String> {
        if transcript.v != TRANSCRIPT_VERSION {
            return Err(format!("transcript version {} is unsupported", transcript.v));
        }
        let mut session = Session::new(SessionOptions {
            library,
            validation: options.clone(),
            input: InputMode::Message,
            prompt_digest: transcript.prompt_digest.clone(),
        });
        for (reference, value) in &transcript.preload {
            if !session.preload_ref(reference, value.clone()) {
                return Err(format!("preload for ref {reference:?} was rejected"));
            }
        }
        let mut events = transcript.events.clone();
        events.sort_by_key(TranscriptEventAt::after);
        Ok(Self {
            session,
            queries: transcript.query_results.clone(),
            transcript,
            events,
            chunk: 0,
            event: 0,
            finished: false,
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    pub fn chunk_index(&self) -> usize {
        self.chunk
    }

    pub fn chunk_count(&self) -> usize {
        self.transcript.chunks.len()
    }

    /// Runs one chunk, then any events due after it and their deliveries.
    pub fn step(&mut self) -> Result<Option<Vec<Update>>, String> {
        if self.chunk < self.transcript.chunks.len() {
            let chunk = self.transcript.chunks[self.chunk].clone();
            let push = self.session.push(chunk.text.as_bytes());
            let mut updates = vec![push];
            updates.extend(settle_updates(
                &mut self.session,
                &updates[0].requests,
                &self.queries,
            )?);
            while self.event < self.events.len()
                && self.events[self.event].after() <= self.chunk
            {
                let event = self.events[self.event].clone();
                let update = event_update(&mut self.session, &event);
                let deliveries = settle_updates(
                    &mut self.session,
                    &update.requests,
                    &self.queries,
                )?;
                updates.push(update);
                updates.extend(deliveries);
                self.event += 1;
            }
            self.chunk += 1;
            return Ok(Some(updates));
        }
        if !self.finished {
            self.finished = true;
            let finish = self.session.finish();
            let deliveries = settle_updates(
                &mut self.session,
                &finish.requests,
                &self.queries,
            )?;
            let mut updates = vec![finish];
            updates.extend(deliveries);
            while self.event < self.events.len() {
                let event = self.events[self.event].clone();
                let update = event_update(&mut self.session, &event);
                let deliveries = settle_updates(
                    &mut self.session,
                    &update.requests,
                    &self.queries,
                )?;
                updates.push(update);
                updates.extend(deliveries);
                self.event += 1;
            }
            return Ok(Some(updates));
        }
        Ok(None)
    }
}

fn phase_name(phase: SessionPhase) -> &'static str {
    match phase {
        SessionPhase::Waiting => "waiting",
        SessionPhase::Streaming => "streaming",
        SessionPhase::Done => "done",
        SessionPhase::Rejected => "rejected",
    }
}

/// Replays and verifies a transcript. The library is supplied by the caller so
/// a stored `lib.mismatch` can still be reported while replaying.
pub fn replay(
    transcript: &Transcript,
    library: Arc<Library>,
    options: &Options,
) -> Result<Recording, String> {
    let expected = transcript
        .expect
        .as_ref()
        .ok_or_else(|| format!("{} has no expectation", transcript.id))?;
    let recording = record(transcript, library, options)?;
    recording.verify(expected)?;
    Ok(recording)
}

/// Scripted reply construction used by the recorder: prose, one fenced
/// program, then optional trailing prose.
pub fn scripted_reply(program: &str) -> String {
    format!("Here is the view.\n\n```ui\n{program}\n```\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use genui_core::r#lib::Library;

    fn library() -> Arc<Library> {
        Arc::new(genui_std::std_library().expect("std"))
    }

    fn simple() -> Transcript {
        let prompt = crate::generate(
            library().spec(),
            &crate::PromptOptions::default(),
        );
        let reply = scripted_reply("root=Column([Text(\"Hello\")])");
        let chunks = reply
            .as_bytes()
            .chunks(8)
            .map(|chunk| TranscriptChunk {
                at_ms: 16,
                text: String::from_utf8_lossy(chunk).into_owned(),
            })
            .collect();
        Transcript {
            v: 1,
            id: "t".into(),
            label: SCRIPTED_LABEL.into(),
            prompt: prompt.text.clone(),
            prompt_digest: prompt.digest,
            source: None,
            chunks,
            events: Vec::new(),
            preload: BTreeMap::new(),
            query_results: BTreeMap::new(),
            expect: None,
        }
    }

    #[test]
    fn a_transcript_records_and_replays_exactly() {
        let mut transcript = simple();
        let recording = record(&transcript, library(), &Options::default()).expect("record");
        assert_eq!(recording.phase, "done");
        assert!(!recording.patches.is_empty());
        transcript.expect = Some(recording.expectation().expect("expect"));
        replay(&transcript, library(), &Options::default()).expect("replay");
    }

    #[test]
    fn a_changed_expectation_is_detected() {
        let mut transcript = simple();
        let recording = record(&transcript, library(), &Options::default()).expect("record");
        let mut expectation = recording.expectation().expect("expect");
        expectation.patches.push("not-a-batch".into());
        transcript.expect = Some(expectation);
        let error = replay(&transcript, library(), &Options::default()).unwrap_err();
        assert!(error.contains("patch stream differs"), "{error}");
    }
}
