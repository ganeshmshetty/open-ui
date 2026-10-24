//! Recorded conversation transcripts (SPEC §12.7, ROADMAP item 17).
//!
//! `transcripts --record` writes the checked-in scripted conversations;
//! `transcripts` or `--verify` replays every file and compares the exact
//! canonical patch stream, state, model events and diagnostics.

use genui_core::{
    ir::{Prop, PropValue},
    r#lib::Library,
    validate::{Options, validate},
    value::Value,
};
use genui_prompt::{
    PromptOptions, generate,
    transcript::{
        SCRIPTED_LABEL, Transcript, TranscriptChunk, TranscriptEventAt, record, scripted_reply,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

/// Twenty deterministic conversations covering data, forms and actions.
const SELECTED: &[&str] = &[
    "01-sales-dashboard",
    "02-operations",
    "03-customer-detail",
    "04-project-detail",
    "05-support",
    "06-team-directory",
    "07-order-table",
    "08-incident-form",
    "10-onboarding-form",
    "12-delivery-form",
    "13-sales-tabs",
    "14-marketing-chart",
    "15-inventory",
    "16-finance",
    "17-plan-comparison",
    "18-release-notes",
    "19-nearby-places",
    "20-task-list",
    "22-usage-details",
    "24-shipping-details",
];

const CHUNK_BYTES: usize = 64;

#[derive(serde::Deserialize)]
struct HostFile {
    tools: Vec<String>,
    refs: Vec<String>,
    url_hosts: Vec<String>,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn capabilities() -> BTreeSet<String> {
    BTreeSet::from(["charts".into(), "images".into(), "markdown".into()])
}

fn host_options() -> Result<Options, String> {
    let path = root().join("conformance/libs/host.json");
    let bytes =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let host: HostFile = serde_json::from_str(&bytes).map_err(|error| error.to_string())?;
    Ok(Options {
        tools: host.tools.into_iter().collect(),
        refs: host.refs.into_iter().collect(),
        url_hosts: host.url_hosts.into_iter().collect(),
        capabilities: capabilities(),
        ..Options::default()
    })
}

fn prompt_options() -> PromptOptions {
    PromptOptions {
        capabilities: capabilities(),
        ..PromptOptions::default()
    }
}

fn sample_values(path: &str) -> Result<BTreeMap<String, Value>, String> {
    let file = root().join(path);
    let bytes =
        std::fs::read_to_string(&file).map_err(|error| format!("{}: {error}", file.display()))?;
    let values: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&bytes).map_err(|error| error.to_string())?;
    Ok(values
        .into_iter()
        .filter_map(|(key, value)| Some((key, serde_json::from_value(value).ok()?)))
        .collect())
}

fn referenced(source: &str, prefix: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = source;
    while let Some(index) = rest.find(prefix) {
        rest = &rest[index + prefix.len()..];
        let Some(end) = rest.find('"') else { break };
        let reference = &rest[..end];
        if !reference.is_empty() && !found.iter().any(|item| item == reference) {
            found.push(reference.to_string());
        }
        rest = &rest[end..];
    }
    found
}

fn chunks(reply: &str) -> Vec<TranscriptChunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for character in reply.chars() {
        current.push(character);
        if current.len() >= CHUNK_BYTES {
            chunks.push(TranscriptChunk {
                at_ms: 16,
                text: std::mem::take(&mut current),
            });
        }
    }
    if !current.is_empty() {
        chunks.push(TranscriptChunk {
            at_ms: 16,
            text: current,
        });
    }
    chunks
}

fn first_button_action(source: &[u8], library: &Library, options: &Options) -> Option<String> {
    let validation = validate(source, library, options);
    for node in validation.ir.nodes.values() {
        let Some(component) = library.components().get(node.kind.0 as usize) else {
            continue;
        };
        if component.name != "Button" {
            continue;
        }
        let Some(index) = component.args.iter().position(|arg| arg.name == "on_click") else {
            continue;
        };
        if matches!(
            node.props
                .get(&genui_core::ir::PropId(u16::try_from(index).ok()?)),
            Some(Prop::Static(PropValue::Action(_)))
        ) {
            return Some(node.path.0.clone());
        }
    }
    None
}

fn one(
    name: &str,
    library: &Arc<Library>,
    options: &Options,
    refs: &BTreeMap<String, Value>,
    queries: &BTreeMap<String, Value>,
) -> Result<Transcript, String> {
    let path = root().join(format!("examples/{name}.ui"));
    let source =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let prompt = generate(library.spec(), &prompt_options());
    let reply = scripted_reply(&source);
    let chunks = chunks(&reply);
    let mut preload = BTreeMap::new();
    for reference in referenced(&source, "ref(\"") {
        if let Some(value) = refs.get(&reference) {
            preload.insert(reference, value.clone());
        }
    }
    let mut query_results = BTreeMap::new();
    for tool in referenced(&source, "tool(\"") {
        if let Some(value) = queries.get(&tool) {
            query_results.insert(tool, value.clone());
        }
    }
    let mut events = Vec::new();
    if let Some(action_path) = first_button_action(source.as_bytes(), library, options) {
        events.push(TranscriptEventAt::Activate {
            after_chunk: chunks.len(),
            path: action_path,
            prop: "on_click".into(),
        });
    }
    let mut transcript = Transcript {
        v: 1,
        id: name.into(),
        label: SCRIPTED_LABEL.into(),
        prompt: prompt.text,
        prompt_digest: prompt.digest,
        source: Some(format!("examples/{name}.ui")),
        chunks,
        events,
        preload,
        query_results,
        expect: None,
    };
    let recording = record(&transcript, library.clone(), options)?;
    transcript.expect = Some(recording.expectation()?);
    Ok(transcript)
}

fn record_all(dir: &Path, library: &Arc<Library>, options: &Options) -> Result<usize, String> {
    let refs = sample_values("conformance/libs/refs.json")?;
    let queries = sample_values("conformance/libs/queries.json")?;
    std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let mut written = 0;
    for name in SELECTED {
        let transcript = one(name, library, options, &refs, &queries)?;
        let json = serde_json::to_string_pretty(&transcript).map_err(|error| error.to_string())?;
        let path = dir.join(format!("{name}.json"));
        std::fs::write(&path, format!("{json}\n"))
            .map_err(|error| format!("{}: {error}", path.display()))?;
        written += 1;
    }
    Ok(written)
}

fn verify_all(dir: &Path, library: &Arc<Library>, options: &Options) -> Result<usize, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    if paths.len() < 20 {
        return Err(format!(
            "expected at least 20 transcripts in {}, found {}",
            dir.display(),
            paths.len()
        ));
    }
    for path in &paths {
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let transcript: Transcript = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if transcript.label != SCRIPTED_LABEL {
            return Err(format!(
                "{}: label {:?} is not {SCRIPTED_LABEL:?}; scripted data must never be presented as a real model run",
                path.display(),
                transcript.label
            ));
        }
        let recording = genui_prompt::transcript::replay(&transcript, library.clone(), options)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        println!(
            "{}: {} batches, {} event(s), {} state slot(s)",
            transcript.id,
            recording.patches.len(),
            recording.events.len(),
            recording.state.values.len()
        );
    }
    Ok(paths.len())
}

pub fn run(args: &[String]) -> Result<bool, String> {
    let mut dir = root().join("eval/transcripts");
    let mut record_mode = false;
    let mut verify_mode = false;
    for arg in args {
        match arg.as_str() {
            "--record" => record_mode = true,
            "--verify" => verify_mode = true,
            other if other.starts_with("--") => {
                return Err(format!("unknown transcripts option {other:?}"));
            }
            other => dir = PathBuf::from(other),
        }
    }
    if record_mode && verify_mode {
        return Err("choose either --record or --verify".into());
    }
    let library =
        Arc::new(genui_std::std_library().map_err(|errors| format!("std build: {errors:?}"))?);
    let options = host_options()?;
    if record_mode {
        let count = record_all(&dir, &library, &options)?;
        println!("recorded {count} scripted transcripts in {}", dir.display());
        return Ok(true);
    }
    let count = verify_all(&dir, &library, &options)?;
    println!("PASS: {count} transcripts replay exactly");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_extraction_finds_refs_and_tools() {
        let source =
            "Table([TableColumn(\"A\",\"a\")],ref(\"t4.orders\"))\nQ=tool(\"sales.by_month\",{})";
        assert_eq!(referenced(source, "ref(\""), vec!["t4.orders"]);
        assert_eq!(referenced(source, "tool(\""), vec!["sales.by_month"]);
    }

    #[test]
    fn chunking_is_utf8_safe() {
        let reply = "é".repeat(40);
        let chunks = chunks(&reply);
        assert!(chunks.iter().all(|chunk| chunk.text.len() <= CHUNK_BYTES));
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.text.as_str())
                .collect::<String>(),
            reply
        );
    }
}
