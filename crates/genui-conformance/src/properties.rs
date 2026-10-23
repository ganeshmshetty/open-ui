//! Seeded, reproducible whole-program streaming property (§12.3 / M3).
use crate::streaming::Oracle;
use genui_core::{canonical, validate::Options};
use std::sync::Arc;
fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
pub fn generated(seed: u64) -> (Vec<u8>, Options, bool) {
    let mut rng = seed + 0x63a9_4b21;
    let n = next(&mut rng);
    let text = format!("case {seed} 😀 {}", n % 100000);
    let quoted = canonical::encode(&serde_json::json!(text));
    let message = seed % 20 == 5;
    let program = match seed % 20 {
        0 => format!("root=Text({quoted})\n"),
        1 => format!("root=Column([a,b,a])\nb=Text({quoted})\na=Divider()\n"),
        2 => format!("root=Input({quoted},bind:$x)\n$x=\"\"\n"),
        3 => format!("a=Row([Divider(),Text({quoted})])\nroot=Column([a,null,a])\n"),
        4 => format!("root=Text({quoted},tone: \"in"),
        5 => format!("Intro {seed}\u{202e}\r\n```ui v1\r\nroot=Text({quoted})\r\n```\r\nAfter\n"),
        6 => format!("value={}+{}\nroot=Stat({quoted},value)\n", n % 100, n % 7),
        7 => "root=Text(text(rows))\nquery rows=tool(\"known\",{},default:[])\n".to_string(),
        8 => format!("root=Text(text(ref(\"t.data\")))\n// {seed}\n"),
        9 => format!("root=a\na=Column([a,Text({quoted})])\n"),
        10 => format!("root=Column(each([1,2],|x|Text({quoted}),key:|x|x))\n"),
        11 => format!("root=Text({quoted})\nroot=Bad()\nunused=Text(\"orphan\")\n"),
        12 => "$z=\"z\"\nroot=Input(\"Name\",bind:$z)\n$a=\"a\"\n".into(),
        13 => "$z=\"z\"\nvalue=$z\nroot=Text(text(value))\n$a=\"a\"\n".into(),
        14 => format!(
            "body=Text({quoted})\nroot=card\ncard=Card(body,title: $x+\" suffix\")\n$x=\"title\"\n"
        ),
        15 => "a=Column([Column([Divider()]),Text(\"kept\")])\nroot=Column([a,a])\n".into(),
        16 => format!("root=Button(\"Go\",on_click:go)\ngo=say({quoted})\n"),
        17 => format!(
            "go=set($x,1)\nroot=Button(\"Go\",on_click:go)\n$x={}\n",
            n % 100
        ),
        18 => format!("root=Text(v)\nv={quoted}.\nquery q=tool(\"known\",{{}},default:[])\n"),
        _ => format!("root=Column([draft,other])\ndraft=Text({quoted}).\nother=Divider()\n"),
    };
    let mut options = Options::default();
    options.tools.insert("known".into());
    options.refs.insert("t.data".into());
    if n.is_multiple_of(5) {
        options.limits.max_nodes = (n % 9) as usize;
    }
    if n.is_multiple_of(7) {
        options.limits.syntax.max_stmt_bytes = [0, 8, 24, 64][(n % 4) as usize];
    }
    if n.is_multiple_of(11) {
        options.limits.syntax.max_program_bytes = (n % 100) as usize;
    }
    if n.is_multiple_of(13) {
        options.limits.syntax.max_expr_depth = (n % 6) as usize;
    }
    if n.is_multiple_of(19) {
        options.limits.max_queries = (n % 3) as usize;
    }
    if n.is_multiple_of(3) {
        options.limits.max_state_bytes = [0, 2, 3, 5, 64][(n % 5) as usize];
    }
    if n.is_multiple_of(17) {
        options.limits.max_ir_depth = (n % 5) as usize;
    }
    (program.into_bytes(), options, message)
}
pub fn run(count: u64) -> Result<(), String> {
    let library = Arc::new(genui_std::std_library().map_err(|e| format!("std: {e:?}"))?);
    for seed in 0..count {
        let (input, options, message) = generated(seed);
        let mut rng = seed + 1;
        let mut parts = Vec::new();
        let mut offset = 0;
        while offset < input.len() {
            let end = (offset + 1 + (next(&mut rng) % 97) as usize).min(input.len());
            parts.push(&input[offset..end]);
            offset = end;
        }
        Oracle::new(&input, library.clone(), options, message)
            .check(&parts)
            .map_err(|e| {
                format!(
                    "generated seed {seed}, input {:?}: {e}",
                    String::from_utf8_lossy(&input)
                )
            })?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn generated_streams_equal_batch() {
        let count = std::env::var("GENUI_STREAM_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(512);
        assert!(count > 0);
        super::run(count).unwrap();
        eprintln!(
            "PASS: {count} generated programs, seeds 0..{count}, stream == batch and replay invariants"
        );
    }
}
