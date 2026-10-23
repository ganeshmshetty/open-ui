//! Mandatory live I1–I7 / G2,G6,G7 checks, shared by fixtures, property and fuzz gates.
use super::*;
use genui_core::{
    Session, SessionOptions,
    r#lib::Library,
    patch::{PatchBatch, Replay, Tree},
    session::InputMode,
    validate::{Options, validate, validate_message},
};
use std::sync::Arc;

pub struct Oracle {
    library: Arc<Library>,
    options: Options,
    mode: InputMode,
    batch: genui_core::validate::Validation,
    trace: Vec<genui_core::syntax::streaming::Seal>,
}
impl Oracle {
    pub fn new(input: &[u8], library: Arc<Library>, options: Options, message: bool) -> Self {
        let batch = if message {
            validate_message(input, &library, &options).1
        } else {
            validate(input, &library, &options)
        };
        let mode = if message {
            InputMode::Message
        } else {
            InputMode::Program
        };
        let mut whole = Session::new(SessionOptions {
            library: library.clone(),
            validation: options.clone(),
            input: mode,
        });
        whole.push(input);
        whole.finish();
        Self {
            library,
            options,
            mode,
            batch,
            trace: whole.seals().to_vec(),
        }
    }
    pub fn check(&self, parts: &[&[u8]]) -> Result<Vec<PatchBatch>> {
        let mut session = Session::new(SessionOptions {
            library: self.library.clone(),
            validation: self.options.clone(),
            input: self.mode,
        });
        let mut replay = Replay::default();
        let mut batches = Vec::new();
        let mut trace = Vec::new();
        let mut prose = BTreeMap::new();
        let mut next_seq = 0;
        for (i, part) in parts.iter().enumerate() {
            let update = session.push(part);
            let batch = update
                .patches
                .as_ref()
                .ok_or("push omitted its atomic batch")?;
            if batch.epoch != 1 || batch.seq != next_seq {
                return Err("ordinary push sequence has a gap or epoch changed".into());
            }
            next_seq += 1;
            check_prose(&session, &update, &mut prose)?;
            live(&session, &update, &mut replay, &mut trace)
                .map_err(|e| format!("chunk {i}: {e}"))?;
            if let Some(batch) = update.patches {
                batches.push(batch);
            }
        }
        let update = session.finish();
        if let Some(batch) = &update.patches
            && (batch.epoch != 1 || batch.seq != next_seq)
        {
            return Err("finish sequence has a gap".into());
        }
        check_prose(&session, &update, &mut prose)?;
        live(&session, &update, &mut replay, &mut trace).map_err(|e| format!("finish: {e}"))?;
        if let Some(batch) = update.patches {
            batches.push(batch);
        }
        if *session.ir() != self.batch.ir {
            return Err(format!(
                "final IR differs from batch\nstream {:?}\nbatch {:?}",
                session.ir(),
                self.batch.ir
            ));
        }
        let mut actual: Vec<_> = session
            .diagnostics()
            .iter()
            .filter(|d| !d.provisional)
            .map(DiagnosticKey::from)
            .collect();
        actual.sort();
        let mut expected: Vec<_> = self
            .batch
            .diagnostics
            .iter()
            .map(DiagnosticKey::from)
            .collect();
        expected.sort();
        if actual != expected {
            return Err(format!(
                "final diagnostics differ\nstream {actual:?}\nbatch {expected:?}"
            ));
        }
        if replay.tree != Tree::from_ir(&self.batch.ir, &self.library) {
            return Err(format!(
                "final patch replay differs from batch tree: {:?} != {:?}",
                replay.tree,
                Tree::from_ir(&self.batch.ir, &self.library)
            ));
        }
        if session.seals() != self.trace {
            return Err(format!(
                "sealed trace depends on chunking\nstream {:?}\nwhole {:?}",
                session.seals(),
                self.trace
            ));
        }
        Ok(batches)
    }
}
pub fn check(
    input: &[u8],
    parts: &[&[u8]],
    library: Arc<Library>,
    options: Options,
    message: bool,
) -> Result<Vec<PatchBatch>> {
    Oracle::new(input, library, options, message).check(parts)
}

fn check_prose(
    session: &Session,
    update: &genui_core::Update,
    prose: &mut BTreeMap<u32, String>,
) -> Result<()> {
    for delta in &update.prose {
        prose
            .entry(delta.segment)
            .or_default()
            .push_str(&delta.append);
    }
    let expected: BTreeMap<_, _> = session
        .segments()
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            if let Segment::Markdown(text) = s {
                Some((i as u32, genui_core::patch::strip_invisibles(text)))
            } else {
                None
            }
        })
        .collect();
    if *prose != expected {
        return Err(format!(
            "prose deltas differ from current segments: {prose:?} vs {expected:?}"
        ));
    }
    Ok(())
}
fn live(
    session: &Session,
    update: &genui_core::Update,
    replay: &mut Replay,
    trace: &mut Vec<genui_core::syntax::streaming::Seal>,
) -> Result<()> {
    let work = session.parser_work();
    if work.parsed_seals > session.seals().len() || work.tail_bytes > work.tail_limit {
        return Err(format!("I7 parse-once/tail bound violated: {work:?}"));
    }
    if session.ir().nodes.values().any(|n| {
        n.status == genui_core::ir::NodeStatus::Partial
            && session
                .seals()
                .iter()
                .filter_map(|s| s.statement.as_ref())
                .any(|s| {
                    s.kind != genui_core::syntax::ast::StatementKind::State && s.name == n.origin
                })
    }) {
        return Err("a sealed origin became provisional".into());
    }
    if !update.requests.is_empty() || !update.model_events.is_empty() {
        return Err("M3 emitted host effects".into());
    }
    if !session.seals().starts_with(trace) {
        return Err("sealed statements changed or disappeared".into());
    }
    if session
        .seals()
        .windows(2)
        .any(|s| s[0].span.end > s[1].span.start)
    {
        return Err("seal source order is not monotonic".into());
    }
    *trace = session.seals().to_vec();
    let states: BTreeMap<_, _> = session
        .seals()
        .iter()
        .filter_map(|s| s.statement.as_ref())
        .filter(|s| s.kind == genui_core::syntax::ast::StatementKind::State)
        .map(|s| (s.name.as_str(), ()))
        .collect();
    if session
        .ir()
        .states
        .keys()
        .any(|s| !states.contains_key(s.as_str()))
    {
        return Err("provisional content declared state".into());
    }
    let queries: BTreeMap<_, _> = session
        .seals()
        .iter()
        .filter_map(|s| s.statement.as_ref())
        .filter(|s| s.kind == genui_core::syntax::ast::StatementKind::Query)
        .map(|s| (s.name.as_str(), ()))
        .collect();
    if session
        .ir()
        .queries
        .keys()
        .any(|s| !queries.contains_key(s.as_str()))
    {
        return Err("provisional content declared a query".into());
    }
    if session.ir().actions.iter().any(|a| {
        session.ir().nodes.get(&a.origin).is_some_and(|n| {
            n.status == genui_core::ir::NodeStatus::Partial
                || session.provisional_origins().contains(&n.origin)
        })
    }) {
        return Err("provisional content created action plans".into());
    }
    if let Some(batch) = &update.patches {
        let report = replay.apply(batch);
        if report.needs_snapshot || report.discarded || !report.errors.is_empty() {
            return Err(format!(
                "invalid patch order/sequence: {report:?}, {batch:?}"
            ));
        }
    }
    if replay.tree != *session.tree() {
        return Err(format!(
            "replay differs from live tree: {:?} vs {:?}",
            replay.tree,
            session.tree()
        ));
    }
    if !session.tree().nodes.is_empty() && session.tree().root.is_none() {
        return Err("tree has nodes without a root".into());
    }
    Ok(())
}

pub(crate) fn run_case(
    path: &Path,
    schemas: &Schemas,
    library: Arc<Library>,
) -> Result<semantic::Checks> {
    let case: Case = checked_json(&path.join("case.json"), &schemas.case)?;
    let input = fs::read(path.join("input.ui")).map_err(|e| e.to_string())?;
    let options = Options {
        limits: serde_json::from_value(
            serde_json::to_value(&case.limits).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
        ..Options::default()
    };
    let message = case.tags.iter().any(|s| s == "message");
    let batch = if message {
        validate_message(&input, &library, &options).1
    } else {
        validate(&input, &library, &options)
    };
    let allowed = [
        "case.json",
        "input.ui",
        "stream.json",
        "want.ir.json",
        "want.diag.json",
        "want.patches.json",
        "checkpoints.json",
        "script.json",
    ];
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|name| allowed.contains(&name))
        {
            return Err(format!(
                "unsupported streaming fixture file {}",
                entry.path().display()
            ));
        }
    }
    let schema: Value = read_json(
        &path
            .parent()
            .and_then(Path::parent)
            .ok_or("fixture root")?
            .join("schemas/ir.schema.json"),
    )?;
    let ir_schema = jsonschema::validator_for(&schema).map_err(|e| e.to_string())?;
    let want: Value = checked_json(&path.join("want.ir.json"), &ir_schema)?;
    if fs::read_to_string(path.join("want.ir.json")).map_err(|e| e.to_string())?
        != genui_core::canonical::encode(&want) + "\n"
    {
        return Err("IR golden is not canonical bytes".into());
    }
    if batch.ir.canonical_tree(&library) != want {
        return Err(format!(
            "final IR golden differs: {}",
            batch.ir.canonical_tree(&library)
        ));
    }
    let mut want_diags: Vec<DiagnosticKey> =
        checked_json(&path.join("want.diag.json"), &schemas.diagnostics)?;
    want_diags.sort();
    let mut actual: Vec<_> = batch.diagnostics.iter().map(DiagnosticKey::from).collect();
    actual.sort();
    if actual != want_diags {
        return Err(format!("diagnostic golden differs: {actual:?}"));
    }
    let explicit: Option<Vec<String>> = if path.join("stream.json").exists() {
        Some(checked_json(&path.join("stream.json"), &schemas.stream)?)
    } else {
        None
    };
    if path.join("want.patches.json").exists() {
        let actual = if path.join("script.json").exists() {
            run_script(path, library.clone(), options.clone(), message)?
        } else {
            let parts = explicit
                .as_ref()
                .ok_or("patch goldens require explicit stream or a session script")?;
            let parts: Vec<_> = parts.iter().map(|s| s.as_bytes()).collect();
            check(&input, &parts, library.clone(), options.clone(), message)?
        };
        let want: Vec<PatchBatch> = read_json(&path.join("want.patches.json"))?;
        if actual != want {
            return Err(format!(
                "patch golden differs\nexpected {want:?}\nactual {actual:?}"
            ));
        }
        let encoded = genui_core::canonical::serialize(&want).map_err(|e| e.to_string())? + "\n";
        if fs::read_to_string(path.join("want.patches.json")).map_err(|e| e.to_string())? != encoded
        {
            return Err("patch golden is not canonical bytes".into());
        }
    }
    if path.join("checkpoints.json").exists() {
        checkpoints(
            path,
            explicit
                .as_deref()
                .ok_or("live checkpoints require explicit stream")?,
            library.clone(),
            options.clone(),
            message,
        )?;
    }
    let oracle = Oracle::new(&input, library.clone(), options.clone(), message);
    let mut count = 0;
    chunks::for_each(&input, explicit.as_deref(), |label, parts| {
        oracle.check(parts).map_err(|e| format!("{label}: {e}"))?;
        count += 1;
        Ok(())
    })?;
    Ok(semantic::Checks {
        chunkings: count,
        permutations: 0,
    })
}

#[derive(Deserialize)]
#[serde(
    tag = "op",
    content = "text",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SessionStep {
    Push(String),
    Finish,
    Snapshot,
    Reset,
}
fn run_script(
    path: &Path,
    library: Arc<Library>,
    options: Options,
    message: bool,
) -> Result<Vec<PatchBatch>> {
    let steps: Vec<SessionStep> = read_json(&path.join("script.json"))?;
    let mut session = Session::new(SessionOptions {
        library: library.clone(),
        validation: options.clone(),
        input: if message {
            InputMode::Message
        } else {
            InputMode::Program
        },
    });
    let mut replay = Replay::default();
    let mut trace = Vec::new();
    let mut source = Vec::new();
    let mut batches = Vec::new();
    for step in steps {
        let finish = matches!(step, SessionStep::Finish);
        let update = match step {
            SessionStep::Push(text) => {
                source.extend_from_slice(text.as_bytes());
                session.push(text.as_bytes())
            }
            SessionStep::Finish => session.finish(),
            SessionStep::Snapshot => session.snapshot(),
            SessionStep::Reset => {
                source.clear();
                trace.clear();
                session.reset()
            }
        };
        live(&session, &update, &mut replay, &mut trace)?;
        if finish {
            let batch = if message {
                validate_message(&source, &library, &options).1
            } else {
                validate(&source, &library, &options)
            };
            if *session.ir() != batch.ir {
                return Err("session-script final IR differs from current epoch batch".into());
            }
        }
        if let Some(batch) = update.patches {
            batches.push(batch);
        }
    }
    Ok(batches)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    chunk: usize,
    id: String,
    #[serde(default)]
    absent: bool,
    prop: Option<String>,
    equals: Option<Value>,
    absent_prop: Option<String>,
    status: Option<genui_core::ir::NodeStatus>,
}
fn checkpoints(
    path: &Path,
    parts: &[String],
    library: Arc<Library>,
    options: Options,
    message: bool,
) -> Result<()> {
    let checks: Vec<Checkpoint> = read_json(&path.join("checkpoints.json"))?;
    if checks.iter().any(|c| c.chunk >= parts.len()) {
        return Err("checkpoint chunk is out of range".into());
    }
    let mut session = Session::new(SessionOptions {
        library,
        validation: options,
        input: if message {
            InputMode::Message
        } else {
            InputMode::Program
        },
    });
    for (i, part) in parts.iter().enumerate() {
        session.push(part.as_bytes());
        for c in checks.iter().filter(|c| c.chunk == i) {
            let node = session.tree().nodes.get(&c.id);
            if c.absent {
                if node.is_some() {
                    return Err(format!("chunk {i}: {} should be absent", c.id));
                }
                continue;
            }
            let node =
                node.ok_or_else(|| format!("chunk {i}: missing checkpoint node {}", c.id))?;
            if c.status.is_some_and(|s| s != node.node.status) {
                return Err(format!("chunk {i}: wrong status for {}", c.id));
            }
            if let Some(prop) = &c.prop
                && node.node.props.get(prop) != c.equals.as_ref()
            {
                return Err(format!(
                    "chunk {i}: wrong {}.{prop}: {:?}",
                    c.id,
                    node.node.props.get(prop)
                ));
            }
            if c.absent_prop
                .as_ref()
                .is_some_and(|p| node.node.props.contains_key(p))
            {
                return Err(format!("chunk {i}: unwanted prop"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> Session {
        Session::new(SessionOptions::program(Arc::new(
            genui_std::std_library().unwrap(),
        )))
    }
    #[test]
    fn explicit_program_mode_does_not_extract_a_model_message() {
        let library = Arc::new(genui_std::std_library().unwrap());
        let input = b"Here is UI:\n```ui\nroot=Divider()\n```\n";
        let mut program = Session::new(SessionOptions::program(library.clone()));
        program.push(input);
        program.finish();
        assert!(
            program
                .diagnostics()
                .iter()
                .any(|d| d.code == genui_core::diag::Code::BadChar)
        );
        assert!(program.segments().is_empty());
        let mut message = Session::new(SessionOptions::new(library));
        message.push(input);
        message.finish();
        assert!(message.diagnostics().is_empty());
        assert!(matches!(&message.segments()[0], Segment::Markdown(s) if s == "Here is UI:\n"));
    }
    #[test]
    fn strings_escapes_numbers_failure_and_new_definition_sealing_are_live() {
        let mut s = session();
        s.push(b"root=Text(\"kept");
        assert_eq!(
            s.tree().nodes["root"].node.props["text"],
            serde_json::json!("kept")
        );
        let before = s.tree().clone();
        s.push(b"\".");
        assert_eq!(
            *s.tree(),
            before,
            "a failed closed tail must keep the display"
        );
        let mut s = session();
        s.push(b"root=Text(\"x\\u{1f");
        assert_eq!(
            s.tree().nodes["root"].node.props["text"],
            serde_json::json!("x")
        );
        s.push(b"600}");
        assert_eq!(
            s.tree().nodes["root"].node.props["text"],
            serde_json::json!("x😀")
        );
        s.push(b"\")\n");
        let first = s.seals().to_vec();
        s.push(b"root=Text(\"duplicate");
        assert_eq!(s.seals(), first);
        assert_eq!(
            s.tree().nodes["root"].node.props["text"],
            serde_json::json!("x😀")
        );
        let mut s = session();
        s.push(b"root=Stat(\"Rate\", 1");
        assert_eq!(
            s.tree().nodes["root"].node.props["value"],
            serde_json::json!({"i":1})
        );
        s.push(b"2.5");
        assert_eq!(
            s.tree().nodes["root"].node.props["value"],
            serde_json::json!({"f":12.5})
        );
        s.finish();
        assert!(
            s.ir()
                .nodes
                .values()
                .all(|n| n.status == genui_core::ir::NodeStatus::Sealed)
        );
    }
    #[test]
    fn provisional_state_queries_and_actions_cannot_escape() {
        let mut s = session();
        s.push(b"$draft=\"x");
        assert!(s.ir().states.is_empty());
        s.push(b"\"\n");
        assert!(s.ir().states.contains_key("draft"));
        s.push(b"query later=tool(\"known\",{},default:[])");
        assert!(s.ir().queries.is_empty());
        let mut s = session();
        s.push(b"root=Button(\"Go\",on_click:say(\"Hi\"))");
        assert!(s.ir().actions.is_empty());
        assert!(!s.tree().nodes["root"].node.props.contains_key("on_click"));
        let update = s.finish();
        assert!(update.requests.is_empty());
        assert!(update.model_events.is_empty());
        assert_eq!(s.ir().actions.len(), 1);
    }
    #[test]
    fn pending_named_colon_does_not_invent_a_positional_argument() {
        let mut s = session();
        s.push(b"root=Text(\"x\",tone:");
        assert!(
            !s.diagnostics()
                .iter()
                .any(|d| d.code == Code::ExtraPositional)
        );
        assert_eq!(
            s.tree().nodes["root"].node.props["tone"],
            serde_json::json!("neutral")
        );
        s.push(b"\"info\")");
        assert_eq!(
            s.tree().nodes["root"].node.props["tone"],
            serde_json::json!("info")
        );
    }
    #[test]
    fn parse_once_counters_and_cutoff_sensitive_diagnostics_are_live() {
        let mut parser =
            genui_core::syntax::streaming::StreamingParser::new(SyntaxLimits::default());
        let first = b"root=Text(\"abc";
        parser.push(first);
        let work = parser.work();
        assert_eq!(work.accepted_bytes, first.len());
        assert_eq!(work.parsed_seals, 0);
        assert_eq!(work.tail_parses, 1);
        assert_eq!(work.tail_bytes, first.len());
        parser.push(b"\")\n");
        let work = parser.work();
        assert_eq!(work.accepted_bytes, first.len() + 3);
        assert_eq!(work.parsed_seals, 1);
        assert_eq!(work.tail_parses, 1);
        assert_eq!(work.tail_bytes, 0);
        let mut s = session();
        s.push(b"a=Text(\"\\x");
        assert!(
            s.diagnostics()
                .iter()
                .any(|d| d.code == Code::BadEscape && d.provisional)
        );
    }
    #[test]
    fn late_state_rebinds_and_canonical_budgets_remove_previously_visible_nodes() {
        let mut s = session();
        s.push(b"root=Input(\"Name\",bind:$x)\n");
        assert!(!s.tree().nodes["root"].node.props.contains_key("bind"));
        assert!(
            s.diagnostics()
                .iter()
                .any(|d| d.code == Code::Undeclared && d.provisional)
        );
        s.push(b"$x=\"\"\n");
        assert_eq!(
            s.tree().nodes["root"].node.props["bind"],
            serde_json::json!({"bind":"$x"})
        );
        assert!(s.dirty().dirty.contains("root"));
        let lib = Arc::new(genui_std::std_library().unwrap());
        let mut opts = SessionOptions::program(lib);
        opts.validation.limits.max_nodes = 2;
        let mut s = Session::new(opts);
        s.push(b"root=Column([a,b])\nb=Divider()\n");
        assert!(s.tree().nodes.contains_key("root/children:@b"));
        let update = s.push(b"a=Divider()\n");
        assert!(!s.tree().nodes.contains_key("root/children:@b"));
        assert!(s.tree().nodes.contains_key("root/children:@a"));
        assert!(
            update
                .patches
                .unwrap()
                .ops
                .iter()
                .any(|op| matches!(op,genui_core::patch::Op::Remove{id}if id=="root/children:@b"))
        );
    }
    #[test]
    fn unchanged_static_subtrees_are_reused_and_do_not_emit_prop_updates() {
        let mut s = session();
        s.push(b"stable=Column([Text(\"stable\")])\nroot=Column([stable,draft])\ndraft=Text(\"a");
        let old = s.dirty().instances["root/children:@stable"].clone();
        let before = s.reused_instances();
        let update = s.push(b"bc");
        assert!(s.reused_instances() > before);
        assert_eq!(s.dirty().instances["root/children:@stable"], old);
        assert!(!s.dirty().dirty.contains("stable"));
        assert!(update.patches.unwrap().ops.iter().all(|op|!matches!(op,genui_core::patch::Op::SetProps{id,..}|genui_core::patch::Op::AppendText{id,..}|genui_core::patch::Op::Remove{id}|genui_core::patch::Op::SetStatus{id,..}if id.starts_with("root/children:@stable"))));
    }
    #[test]
    fn dependency_revisions_include_both_binary_and_index_operands() {
        let mut s = session();
        s.push(b"left=2\nright=3\nrows=[1,2]\ni=0\ntotal=left+right\nitem=rows[i]\nroot=Column([Stat(\"Total\",total),Stat(\"Item\",item)])\n");
        assert_eq!(s.dirty().reverse["left"], ["total".into()].into());
        assert_eq!(s.dirty().reverse["right"], ["total".into()].into());
        assert_eq!(s.dirty().reverse["rows"], ["item".into()].into());
        assert_eq!(s.dirty().reverse["i"], ["item".into()].into());
        let revision = &s.dirty().instances["root"];
        assert_eq!(revision.origin_rev, 1);
        assert_eq!(
            revision.dep_revs,
            ["left", "right", "rows", "i", "total", "item"]
                .into_iter()
                .map(|n| (n.to_string(), 1))
                .collect()
        );
    }
    #[test]
    fn snapshot_reset_and_diagnostic_deltas_have_no_sequence_gaps() {
        let mut s = session();
        let mut r = Replay::default();
        let u = s.push(b"root=Text(\"abc\")\n");
        assert!(!r.apply(&u.patches.unwrap()).needs_snapshot);
        let u = s.snapshot();
        assert!(!r.apply(&u.patches.unwrap()).needs_snapshot);
        assert_eq!(r.tree, *s.tree());
        s.finish();
        assert!(s.finish().patches.is_none());
        let u = s.reset();
        let b = u.patches.unwrap();
        assert_eq!(b.epoch, 2);
        assert_eq!(b.seq, 0);
        assert!(!r.apply(&b).needs_snapshot);
        assert!(s.tree().nodes.is_empty());
        assert!(s.seals().is_empty());
        let mut s = session();
        let first = s.push(b"bad=Unknown()\n");
        assert!(!first.diagnostics.is_empty());
        assert!(s.push(b" ").diagnostics.is_empty());
    }
    #[test]
    fn sealing_an_overlay_updates_its_retained_body() {
        let input = b"root=Column([later])\nlater=Text(\"Ready\")\n";
        let oracle = Oracle::new(
            input,
            Arc::new(genui_std::std_library().unwrap()),
            Options::default(),
            false,
        );
        oracle
            .check(&[
                b"root=C",
                b"olumn([late",
                b"r])\nlater=Text(\"Read",
                b"y\")\n",
            ])
            .unwrap();
    }
    #[test]
    fn a_visible_draft_updates_without_visiting_unchanged_siblings() {
        let mut s = session();
        let mut input = String::new();
        for i in 0..998 {
            input.push_str(&format!("n{i}=Divider()\n"));
        }
        input.push_str("root=Column([draft,");
        input.push_str(
            &(0..998)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        input.push_str("])\ndraft=Text(\"a");
        s.push(input.as_bytes());
        assert_eq!(s.ir().nodes.len(), 1000);
        let before = s.maintenance_work();
        let update = s.push(b"b");
        let after = s.maintenance_work();
        assert!(
            after.statements_checked - before.statements_checked <= 4,
            "{before:?} -> {after:?}"
        );
        assert!(
            after.instances_examined - before.instances_examined <= 4,
            "{before:?} -> {after:?}"
        );
        assert_eq!(
            after.wire_nodes_examined - before.wire_nodes_examined,
            1,
            "{before:?} -> {after:?}"
        );
        assert_eq!(
            update.patches.unwrap().ops,
            vec![genui_core::patch::Op::AppendText {
                id: "root/children:@draft".into(),
                prop: "text".into(),
                text: "b".into()
            }]
        );
    }
    #[test]
    fn an_earlier_dynamic_does_not_rebuild_existing_siblings() {
        let mut s = session();
        let mut input = String::from("$x=\"\"\nstable=Text($x+\"old\")\n");
        for i in 0..997 {
            input.push_str(&format!("n{i}=Divider()\n"));
        }
        input.push_str("root=Column([late,stable,");
        input.push_str(
            &(0..997)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        input.push_str("])\n");
        s.push(input.as_bytes());
        let nodes = s.ir().nodes.clone();
        let (id, stable) = nodes
            .iter()
            .find(|(_, n)| n.path.0 == "root/children:@stable")
            .unwrap();
        let before = s.maintenance_work();
        let update = s.push(b"late=Text($x+\"new\")\n");
        let after = s.maintenance_work();
        assert!(
            after.instances_examined - before.instances_examined <= 8,
            "{before:?} -> {after:?}"
        );
        assert!(
            after.wire_nodes_examined - before.wire_nodes_examined <= 3,
            "{before:?} -> {after:?}"
        );
        assert!(std::ptr::eq(stable, s.ir().nodes.get(id).unwrap()));
        assert_eq!(update.patches.unwrap().ops.len(), 1);
    }
    #[test]
    fn declaring_a_late_typed_state_rechecks_only_its_binding() {
        let mut s = session();
        let mut input = String::from("field=Input(\"Name\",bind:$late)\n");
        for i in 0..998 {
            input.push_str(&format!("n{i}=Divider()\n"));
        }
        input.push_str("root=Column([field,");
        input.push_str(
            &(0..998)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        input.push_str("])\n");
        s.push(input.as_bytes());
        let before = s.maintenance_work();
        let update = s.push(b"$late=\"\"\n");
        let after = s.maintenance_work();
        assert!(
            after.instances_examined - before.instances_examined <= 4,
            "{before:?} -> {after:?}"
        );
        assert_eq!(after.wire_nodes_examined - before.wire_nodes_examined, 1);
        assert_eq!(
            s.tree().nodes["root/children:@field"].node.props["bind"],
            serde_json::json!({"bind":"$late"})
        );
        assert_eq!(update.patches.unwrap().ops.len(), 1);
    }
    #[test]
    fn resolving_a_late_child_does_not_visit_unchanged_siblings() {
        let mut s = session();
        let mut input = String::new();
        for i in 0..998 {
            input.push_str(&format!("n{i}=Divider()\n"));
        }
        input.push_str("root=Column([late,");
        input.push_str(
            &(0..998)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        input.push_str("])\n");
        s.push(input.as_bytes());
        assert_eq!(s.ir().nodes.len(), 999);
        let before = s.maintenance_work();
        let update = s.push(b"late=Divider()\n");
        let after = s.maintenance_work();
        assert_eq!(s.ir().nodes.len(), 1000);
        assert!(
            after.instances_examined - before.instances_examined <= 8,
            "{before:?} -> {after:?}"
        );
        assert!(
            after.wire_nodes_examined - before.wire_nodes_examined <= 3,
            "{before:?} -> {after:?}"
        );
        assert_eq!(update.patches.unwrap().ops.len(), 1);
    }
    #[test]
    fn sealing_and_finishing_a_visible_draft_keep_work_local() {
        let mut s = session();
        let mut input = String::new();
        for i in 0..998 {
            input.push_str(&format!("n{i}=Divider()\n"));
        }
        input.push_str("root=Column([draft,");
        input.push_str(
            &(0..998)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        input.push_str("])\ndraft=Text(\"a");
        s.push(input.as_bytes());
        let before = s.maintenance_work();
        s.push(b"b\")\n");
        let sealed = s.maintenance_work();
        assert!(
            sealed.instances_examined - before.instances_examined <= 4,
            "{before:?} -> {sealed:?}"
        );
        assert_eq!(sealed.wire_nodes_examined - before.wire_nodes_examined, 1);
        s.finish();
        let after = s.maintenance_work();
        assert_eq!(after.instances_examined, sealed.instances_examined);
        assert_eq!(after.wire_nodes_examined, sealed.wire_nodes_examined);
        assert_eq!(s.ir().nodes.len(), 1000);
    }
    #[test]
    fn an_unrecognized_prefix_does_not_rescan_provisional_diagnostics() {
        let mut s = session();
        s.push("@".repeat(1000).as_bytes());
        assert!(s.parser_work().tail_bytes <= 1000);
        let before = s.maintenance_work();
        let update = s.push(b"@");
        let after = s.maintenance_work();
        assert!(
            after.diagnostics_examined - before.diagnostics_examined <= 8,
            "{before:?} -> {after:?}"
        );
        assert_eq!(update.diagnostics.len(), 1);
        assert_eq!(update.diagnostics[0].code, Code::BadChar);
        assert!(update.diagnostics[0].provisional);
    }
    #[test]
    fn scanner_diagnostic_history_is_not_rescanned_for_new_chunks() {
        let mut s = session();
        let mut input = String::new();
        for i in 0..1000 {
            input.push_str(&format!("v{i}=1 &\n"));
        }
        input.push_str("root=Divider()\naside=Text(\"a");
        s.push(input.as_bytes());
        assert!(
            s.diagnostics()
                .iter()
                .filter(|d| d.code == Code::BadChar)
                .count()
                >= 1000
        );
        let before = s.maintenance_work();
        let update = s.push(b"b");
        let after = s.maintenance_work();
        assert!(
            after.diagnostics_examined - before.diagnostics_examined <= 4,
            "{before:?} -> {after:?}"
        );
        assert!(update.diagnostics.is_empty());
    }
    #[test]
    fn a_container_title_does_not_revisit_its_thousand_descendants() {
        let mut s = session();
        let mut input = String::from("body=Column([");
        input.push_str(&vec!["Divider()"; 999].join(","));
        input.push_str("])\nroot=card\ncard=Card(body,title:\"a");
        s.push(input.as_bytes());
        assert_eq!(s.ir().nodes.len(), 1001);
        let before = s.maintenance_work();
        s.push(b"b");
        let after = s.maintenance_work();
        assert!(
            after.statements_checked - before.statements_checked <= 3,
            "{before:?} -> {after:?}"
        );
        assert!(
            after.instances_examined - before.instances_examined <= 2,
            "{before:?} -> {after:?}"
        );
        assert_eq!(
            after.wire_nodes_examined - before.wire_nodes_examined,
            1,
            "{before:?} -> {after:?}"
        );
    }
    #[test]
    fn a_late_query_rechecks_a_failed_leaf_without_scanning_history() {
        let mut opts = SessionOptions::program(Arc::new(genui_std::std_library().unwrap()));
        opts.validation.tools.insert("known".into());
        let mut s = Session::new(opts);
        let mut input = String::new();
        for i in 0..1000 {
            input.push_str(&format!("unused{i}=1\n"));
        }
        input.push_str("root=Text(text(rows))\n");
        s.push(input.as_bytes());
        assert!(s.ir().root.is_none());
        let before = s.maintenance_work();
        s.push(b"query rows=tool(\"known\",{},default:\"ok\")\n");
        let after = s.maintenance_work();
        assert!(s.ir().root.is_some());
        assert!(
            after.statements_checked - before.statements_checked < 100,
            "{before:?} -> {after:?}"
        );
        assert!(
            after.diagnostics_examined - before.diagnostics_examined < 100,
            "{before:?} -> {after:?}"
        );
        assert!(
            after.instances_examined - before.instances_examined <= 3,
            "{before:?} -> {after:?}"
        );
        // Construction, diff and root attachment each charge the one new node.
        assert!(after.wire_nodes_examined - before.wire_nodes_examined <= 3);
    }
    #[test]
    fn a_slot_leaf_can_appear_and_disappear_without_visiting_999_siblings() {
        let mut opts = SessionOptions::program(Arc::new(genui_std::std_library().unwrap()));
        opts.validation.tools.insert("known".into());
        opts.validation.limits.max_queries = 1;
        let mut s = Session::new(opts);
        let mut input = String::new();
        for i in 0..999 {
            input.push_str(&format!("n{i}=Divider()\n"));
        }
        input.push_str("needle=Text(text(rows))\nroot=Column([");
        input.push_str(
            &(0..999)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        input.push_str(",needle])\n");
        s.push(input.as_bytes());
        assert_eq!(s.ir().nodes.len(), 1000);
        for (bytes, nodes) in [
            (
                b"query rows=tool(\"known\",{},default:\"ok\")\n".as_slice(),
                1001,
            ),
            (b"query a=tool(\"known\",{},default:[])\n".as_slice(), 1000),
        ] {
            let before = s.maintenance_work();
            let update = s.push(bytes);
            let ops = &update.patches.as_ref().unwrap().ops;
            assert_eq!(ops.len(), 1);
            if nodes == 1001 {
                assert!(
                    matches!(&ops[0],genui_core::patch::Op::Insert { parent: Some(parent), index:999, node,.. }
                    if parent == "root" && node.id == "root/children:@needle")
                );
            } else {
                assert!(
                    matches!(&ops[0],genui_core::patch::Op::Remove { id } if id == "root/children:@needle")
                );
            }
            let after = s.maintenance_work();
            assert_eq!(s.ir().nodes.len(), nodes);
            assert!(
                after.statements_checked - before.statements_checked < 100,
                "{before:?} -> {after:?}"
            );
            assert!(
                after.instances_examined - before.instances_examined <= 8,
                "{before:?} -> {after:?}"
            );
            assert!(
                after.wire_nodes_examined - before.wire_nodes_examined <= 8,
                "{before:?} -> {after:?}"
            );
            assert!(
                after.diagnostics_examined - before.diagnostics_examined < 100,
                "{before:?} -> {after:?}"
            );
        }
    }
    #[test]
    fn the_first_provisional_slot_leaf_keeps_999_unrelated_siblings() {
        let mut s = session();
        let mut input = String::new();
        for i in 0..999 {
            input.push_str(&format!("n{i}=Divider()\n"));
        }
        input.push_str("root=Column([");
        input.push_str(
            &(0..999)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        input.push_str(",needle])\n");
        s.push(input.as_bytes());
        for bytes in [b"needle=Text(\"a".as_slice(), b"b\")\n".as_slice()] {
            let before = s.maintenance_work();
            s.push(bytes);
            let after = s.maintenance_work();
            assert_eq!(s.ir().nodes.len(), 1001);
            assert!(
                after.statements_checked - before.statements_checked < 100,
                "{before:?} -> {after:?}"
            );
            assert!(
                after.instances_examined - before.instances_examined < 10,
                "{before:?} -> {after:?}"
            );
            assert!(
                after.wire_nodes_examined - before.wire_nodes_examined <= 2,
                "{before:?} -> {after:?}"
            );
        }
    }
    #[test]
    fn action_alias_checks_do_not_visit_unchanged_definition_history() {
        let mut s = session();
        let mut input = String::new();
        for i in 0..1000 {
            input.push_str(&format!("unused{i}=1\n"));
        }
        input.push_str("root=Button(\"Go\",on_click:go)\n");
        s.push(input.as_bytes());
        for (bytes, actions) in [(b"go=say(\"Hi".as_slice(), 0), (b"\")\n".as_slice(), 1)] {
            let before = s.maintenance_work();
            let update = s.push(bytes);
            let after = s.maintenance_work();
            assert!(update.requests.is_empty());
            assert!(update.model_events.is_empty());
            assert_eq!(s.ir().actions.len(), actions);
            assert!(
                after.statements_checked - before.statements_checked < 100,
                "{before:?} -> {after:?}"
            );
            assert!(
                after.instances_examined - before.instances_examined <= 3,
                "{before:?} -> {after:?}"
            );
            assert!(
                after.wire_nodes_examined - before.wire_nodes_examined <= 1,
                "{before:?} -> {after:?}"
            );
        }
    }
    #[test]
    fn query_budget_insertion_skips_unchanged_definitions_and_nodes() {
        let mut opts = SessionOptions::program(Arc::new(genui_std::std_library().unwrap()));
        opts.validation.tools.insert("known".into());
        let mut s = Session::new(opts);
        let mut input = String::from("root=Divider()\n");
        for i in 0..1000 {
            input.push_str(&format!("query s{i:04}=tool(\"known\",{{}},default:[])\n"));
        }
        s.push(input.as_bytes());
        assert_eq!(s.ir().queries.len(), 8);
        let before = s.maintenance_work();
        let u = s.push(b"query a=tool(\"known\",{},default:[])\n");
        let after = s.maintenance_work();
        assert!(u.requests.is_empty());
        assert_eq!(s.ir().queries.len(), 8);
        assert!(s.ir().queries.contains_key("a"));
        assert!(!s.ir().queries.contains_key("s0007"));
        assert!(
            after.statements_checked - before.statements_checked < 100,
            "{before:?} -> {after:?}"
        );
        assert!(
            after.diagnostics_examined - before.diagnostics_examined < 100,
            "{before:?} -> {after:?}"
        );
        assert_eq!(after.instances_examined, before.instances_examined);
        assert_eq!(after.wire_nodes_examined, before.wire_nodes_examined);
    }
    #[test]
    fn state_budget_insertion_rechecks_only_its_frontier() {
        let mut opts = SessionOptions::program(Arc::new(genui_std::std_library().unwrap()));
        opts.validation.limits.max_state_bytes = 3000;
        let mut s = Session::new(opts);
        let mut input = String::new();
        for i in 0..1000 {
            input.push_str(&format!("$s{i:04}=\"x\"\n"));
        }
        input.push_str("root=Divider()\n");
        s.push(input.as_bytes());
        assert_eq!(s.ir().states.len(), 1000);
        let before = s.maintenance_work();
        s.push(b"$a=\"a\"\n");
        let after = s.maintenance_work();
        assert_eq!(s.ir().states.len(), 1000);
        assert!(s.ir().states.contains_key("a"));
        assert!(!s.ir().states.contains_key("s0999"));
        assert!(
            after.statements_checked - before.statements_checked < 100,
            "{before:?} -> {after:?}"
        );
        assert_eq!(after.instances_examined, before.instances_examined);
        assert_eq!(after.wire_nodes_examined, before.wire_nodes_examined);
    }
    #[test]
    fn extraction_diagnostic_history_is_retained_without_materialization() {
        let lib = Arc::new(genui_std::std_library().unwrap());
        let mut s = Session::new(SessionOptions::new(lib));
        let mut input = String::from("```ui\nroot=Divider()\n```\n");
        for _ in 0..1000 {
            input.push_str("```ui\nroot=Divider()\n```\n");
        }
        s.push(input.as_bytes());
        assert_eq!(
            s.diagnostics()
                .iter()
                .filter(|d| d.code == Code::ExtraProgram)
                .count(),
            1000
        );
        let before = s.maintenance_work();
        assert!(s.push(b"prose").diagnostics.is_empty());
        let after = s.maintenance_work();
        assert!(
            after.diagnostics_examined - before.diagnostics_examined <= 4,
            "{before:?} -> {after:?}"
        );
    }
    #[test]
    fn diagnostic_history_is_not_rescanned_for_an_unrelated_draft() {
        let mut s = session();
        let mut input = String::from("root=Divider()\n");
        for i in 0..1000 {
            input.push_str(&format!("unused{i}=Unknown()\n"));
        }
        input.push_str("aside=Text(\"a");
        s.push(input.as_bytes());
        assert!(s.diagnostics().len() >= 2000);
        let before = s.maintenance_work();
        let update = s.push(b"b");
        let after = s.maintenance_work();
        assert!(
            after.diagnostics_examined - before.diagnostics_examined <= 4,
            "{before:?} -> {after:?}"
        );
        assert_eq!(update.diagnostics.len(), 0);
        assert!(s.diagnostics().len() >= 2000);
    }
    #[test]
    fn maintenance_work_does_not_scale_with_an_unchanged_tree() {
        let mut s = session();
        let mut input = String::new();
        for i in 0..999 {
            input.push_str(&format!("n{i}=Divider()\n"));
        }
        input.push_str("root=Column([");
        input.push_str(
            &(0..999)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        input.push_str("])\naside=Text(\"a");
        s.push(input.as_bytes());
        assert_eq!(s.ir().nodes.len(), 1000);
        let before = s.maintenance_work();
        assert!(before.statements_checked >= 1000);
        assert!(before.instances_examined >= 1000);
        assert!(before.wire_nodes_examined >= 1000);
        s.push(b"b");
        let after = s.maintenance_work();
        assert!(
            after.statements_checked - before.statements_checked <= 2,
            "{before:?} -> {after:?}"
        );
        // The new orphan template itself is checked once; no displayed instance is visited.
        assert!(
            after.instances_examined - before.instances_examined <= 1,
            "{before:?} -> {after:?}"
        );
        assert_eq!(after.wire_nodes_examined - before.wire_nodes_examined, 0);
        assert_eq!(after.props_compared - before.props_compared, 0);
    }
}

#[cfg(test)]
mod binding_work_tests {
    use super::*;
    #[test]
    fn ambiguous_state_inference_does_not_rescan_unrelated_siblings() {
        let mut s = Session::new(SessionOptions::program(Arc::new(
            genui_std::std_library().unwrap(),
        )));
        let mut input = String::from("$v=null\nfield=Input(\"Name\",bind:$v)\n");
        for i in 0..998 {
            input.push_str(&format!("n{i}=Divider()\n"));
        }
        input.push_str("root=Column([field,aside,");
        input.push_str(
            &(0..998)
                .map(|i| format!("n{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        input.push_str("])\naside=Text(\"a");
        s.push(input.as_bytes());
        assert_eq!(s.ir().states["v"].ty, genui_core::r#lib::T::Text);
        let before = s.maintenance_work();
        s.push(b"b");
        let after = s.maintenance_work();
        assert!(
            after.statements_checked - before.statements_checked <= 3,
            "{before:?} -> {after:?}"
        );
        assert!(
            after.instances_examined - before.instances_examined <= 3,
            "{before:?} -> {after:?}"
        );
    }
}

#[cfg(test)]
mod fuzz_regressions {
    use super::*;
    /// Retained argument diagnostics must be replayed when a rebuilt task skips
    /// re-binding an unchanged instance. Fuzz artifact: crash-bcedeac5f26e2c65.
    #[test]
    fn retained_argument_diagnostics_survive_a_rebuilt_root_task() {
        let data: &[u8] = &[
            114, 111, 111, 116, 61, 67, 111, 108, 117, 109, 110, 40, 91, 97, 44, 98, 44, 97, 93,
            57, 10, 98, 61, 68, 105, 118, 105, 100, 101, 114, 60, 0, 40, 41, 10, 3, 146, 73, 36,
            146, 73, 13, 61, 36,
        ];
        let seed = data.iter().fold(0x32751u64, |s, b| {
            s.wrapping_mul(31).wrapping_add(*b as u64)
        });
        let mut options = Options::default();
        options.limits.syntax.max_stmt_bytes = (seed % 1024) as usize;
        options.limits.syntax.max_expr_depth = (seed % 49) as usize;
        options.limits.max_nodes = (seed % 100) as usize;
        options.limits.max_ir_depth = (seed % 25) as usize;
        options.limits.max_state_bytes = (seed % 1024) as usize;
        options.limits.max_queries = (seed % 9) as usize;
        options.limits.syntax.max_program_bytes = (seed % 32769) as usize;
        options.tools.insert("known".into());
        options.refs.insert("t.data".into());
        let parts = [&data[..32], &data[32..]];
        for message in [false, true] {
            crate::streaming::check(
                data,
                &parts,
                Arc::new(genui_std::std_library().unwrap()),
                options.clone(),
                message,
            )
            .unwrap();
        }
    }
}
