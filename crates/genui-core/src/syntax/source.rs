//! UTF-8 repair, initial BOM removal and CR normalization shared by all scanners.
#[derive(Default)]
pub(crate) struct Source {
    pending: Vec<u8>,
    started: bool,
    held_cr: bool,
    pub bad_utf8: bool,
}

impl Source {
    pub fn push(&mut self, bytes: &[u8], finish: bool) -> String {
        self.pending.extend_from_slice(bytes);
        let mut decoded = String::new();
        let mut offset = 0;
        while offset < self.pending.len() {
            match std::str::from_utf8(&self.pending[offset..]) {
                Ok(valid) => {
                    decoded.push_str(valid);
                    offset = self.pending.len();
                }
                Err(error) => {
                    let end = offset + error.valid_up_to();
                    // This prefix is guaranteed valid by Utf8Error.
                    if let Ok(valid) = std::str::from_utf8(&self.pending[offset..end]) {
                        decoded.push_str(valid);
                    }
                    offset = end;
                    if let Some(len) = error.error_len() {
                        decoded.push('\u{fffd}');
                        self.bad_utf8 = true;
                        offset += len;
                    } else if finish {
                        decoded.push('\u{fffd}');
                        self.bad_utf8 = true;
                        offset = self.pending.len();
                    } else {
                        break;
                    }
                }
            }
        }
        self.pending.drain(..offset);
        let mut normalized = String::new();
        for ch in decoded.chars() {
            if !self.started {
                self.started = true;
                if ch == '\u{feff}' {
                    continue;
                }
            }
            if self.held_cr {
                normalized.push('\n');
                self.held_cr = false;
                if ch == '\n' {
                    continue;
                }
            }
            if ch == '\r' {
                self.held_cr = true;
            } else {
                normalized.push(ch);
            }
        }
        if finish && self.held_cr {
            normalized.push('\n');
            self.held_cr = false;
        }
        normalized
    }
}
