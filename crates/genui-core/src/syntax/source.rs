//! UTF-8 repair, initial BOM removal and CR normalization shared by all scanners.
#[derive(Default)]
pub(crate) struct Source {
    pending: Vec<u8>,
    started: bool,
    held_cr: Option<usize>,
    pub bad_utf8: bool,
    raw_decoded: usize,
    tracking: bool,
    raw_ends: Vec<usize>,
}
impl Source {
    /// Extraction needs raw byte endpoints to exclude fences without normalizing
    /// away bytes before the program-size budget. Ordinary lexers need no map.
    pub fn tracked() -> Self {
        Self {
            tracking: true,
            ..Self::default()
        }
    }
    pub fn take_raw_ends(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.raw_ends)
    }
    pub fn push(&mut self, bytes: &[u8], finish: bool) -> String {
        self.pending.extend_from_slice(bytes);
        let mut decoded = String::new();
        let mut ends = Vec::new();
        let mut offset = 0;
        while offset < self.pending.len() {
            match std::str::from_utf8(&self.pending[offset..]) {
                Ok(valid) => {
                    decoded.push_str(valid);
                    if self.tracking {
                        let mut cursor = self.raw_decoded + offset;
                        for ch in valid.chars() {
                            cursor += ch.len_utf8();
                            ends.push(cursor);
                        }
                    }
                    offset = self.pending.len();
                }
                Err(error) => {
                    let end = offset + error.valid_up_to();
                    if let Ok(valid) = std::str::from_utf8(&self.pending[offset..end]) {
                        decoded.push_str(valid);
                        if self.tracking {
                            let mut cursor = self.raw_decoded + offset;
                            for ch in valid.chars() {
                                cursor += ch.len_utf8();
                                ends.push(cursor);
                            }
                        }
                    }
                    offset = end;
                    if let Some(len) = error.error_len() {
                        decoded.push('\u{fffd}');
                        self.bad_utf8 = true;
                        offset += len;
                        if self.tracking {
                            ends.push(self.raw_decoded + offset);
                        }
                    } else if finish {
                        decoded.push('\u{fffd}');
                        self.bad_utf8 = true;
                        offset = self.pending.len();
                        if self.tracking {
                            ends.push(self.raw_decoded + offset);
                        }
                    } else {
                        break;
                    }
                }
            }
        }
        self.raw_decoded += offset;
        self.pending.drain(..offset);
        let mut normalized = String::new();
        let mut ends = ends.into_iter();
        self.raw_ends.clear();
        for ch in decoded.chars() {
            let end = if self.tracking {
                ends.next().unwrap_or(self.raw_decoded)
            } else {
                0
            };
            if !self.started {
                self.started = true;
                if ch == '\u{feff}' {
                    continue;
                }
            }
            if let Some(cr_end) = self.held_cr.take() {
                normalized.push('\n');
                if self.tracking {
                    self.raw_ends.push(if ch == '\n' { end } else { cr_end });
                }
                if ch == '\n' {
                    continue;
                }
            }
            if ch == '\r' {
                self.held_cr = Some(end);
            } else {
                normalized.push(ch);
                if self.tracking {
                    self.raw_ends.push(end);
                }
            }
        }
        if finish && let Some(end) = self.held_cr.take() {
            normalized.push('\n');
            if self.tracking {
                self.raw_ends.push(end);
            }
        }
        normalized
    }
}
