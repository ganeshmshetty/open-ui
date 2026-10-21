//! Deterministic byte chunkings from SPEC §12.3; arbitrary UTF-8 splits are intentional.
use crate::Result;

/// Visits one chunking at a time so two-way splits do not require quadratic storage.
pub fn for_each(
    input: &[u8],
    explicit: Option<&[String]>,
    mut visit: impl FnMut(&str, &[&[u8]]) -> Result<()>,
) -> Result<()> {
    visit("batch", &[input])?;
    let bytes: Vec<_> = input.chunks(1).collect();
    visit("byte-by-byte", &bytes)?;
    if input.len() <= 2048 {
        for split in 0..=input.len() {
            visit(
                &format!("split-{split}"),
                &[&input[..split], &input[split..]],
            )?;
        }
    }
    for seed in 1..=200 {
        let mut state = seed as u64;
        let mut parts = Vec::new();
        let mut offset = 0;
        while offset < input.len() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let end = (offset + 1 + (state % 97) as usize).min(input.len());
            parts.push(&input[offset..end]);
            offset = end;
        }
        visit(&format!("random-{seed}"), &parts)?;
    }
    let lines: Vec<_> = input.split_inclusive(|&b| b == b'\n').collect();
    visit("lines", &lines)?;
    if let Some(explicit) = explicit {
        let parts: Vec<_> = explicit.iter().map(|s| s.as_bytes()).collect();
        if parts.concat() != input {
            return Err("stream.json chunks do not concatenate to input.ui".into());
        }
        visit("explicit", &parts)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_chunkings_preserve_bytes_and_include_internal_utf8_and_crlf_splits() {
        let input = "é\r\n".as_bytes();
        let mut labels = Vec::new();
        let mut boundaries = Vec::new();
        for_each(
            input,
            Some(&["é\r".into(), "\n".into()]),
            |label, parts| {
                assert_eq!(parts.concat(), input);
                labels.push(label.to_owned());
                if label.starts_with("split-") {
                    boundaries.push(parts[0].len());
                }
                Ok(())
            },
        )
        .unwrap();
        assert!(boundaries.contains(&1));
        assert!(boundaries.contains(&3));
        assert_eq!(labels.len(), 209);
        assert_eq!(
            labels.iter().filter(|l| l.starts_with("random-")).count(),
            200
        );
    }

    #[test]
    fn explicit_stream_must_preserve_source() {
        assert!(for_each(b"a", Some(&["b".into()]), |_, _| Ok(())).is_err());
    }

    #[test]
    fn large_inputs_skip_two_way_splits() {
        let input = vec![b'a'; 2049];
        let mut count = 0;
        for_each(&input, None, |label, parts| {
            assert!(!label.starts_with("split-"));
            assert_eq!(parts.concat(), input);
            count += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 203);
    }
}
