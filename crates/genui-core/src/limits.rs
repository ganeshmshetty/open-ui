//! Syntax limits needed by M1; semantic/runtime limits are added in M2/M4.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SyntaxLimits {
    pub max_program_bytes: usize,
    pub max_stmt_bytes: usize,
    pub max_stmt_count: usize,
    pub max_bracket_depth: usize,
    pub max_expr_depth: usize,
    pub max_string_bytes: usize,
}
impl Default for SyntaxLimits {
    fn default() -> Self {
        Self {
            max_program_bytes: 256 * 1024,
            max_stmt_bytes: 16 * 1024,
            max_stmt_count: 2000,
            max_bracket_depth: 32,
            max_expr_depth: 48,
            max_string_bytes: 64 * 1024,
        }
    }
}
impl SyntaxLimits {
    pub(crate) fn bounded(mut self) -> Self {
        self.max_program_bytes = self.max_program_bytes.min(4 * 1024 * 1024);
        self.max_stmt_bytes = self.max_stmt_bytes.min(64 * 1024);
        self.max_stmt_count = self.max_stmt_count.min(20_000);
        self.max_bracket_depth = self.max_bracket_depth.min(64);
        self.max_expr_depth = self.max_expr_depth.min(96);
        self.max_string_bytes = self.max_string_bytes.min(1024 * 1024);
        self
    }
}
