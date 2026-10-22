//! Syntax, semantic and runtime limit configuration (SPEC §7.1).
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

/// Complete host limits, bounded before any allocation (§7.1).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Limits {
    #[serde(flatten)]
    pub syntax: SyntaxLimits,
    pub max_ir_depth: usize,
    pub max_nodes: usize,
    pub fuel_per_prop: usize,
    pub max_list_len: usize,
    pub each_max_items: usize,
    pub max_state_bytes: usize,
    pub max_queries: usize,
    pub max_concurrent_requests: usize,
    pub max_ref_resolutions: usize,
    pub max_resolved_data_bytes: usize,
    pub max_event_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            syntax: SyntaxLimits::default(),
            max_ir_depth: 24,
            max_nodes: 10_000,
            fuel_per_prop: 5000,
            max_list_len: 10_000,
            each_max_items: 2000,
            max_state_bytes: 256 * 1024,
            max_queries: 8,
            max_concurrent_requests: 4,
            max_ref_resolutions: 64,
            max_resolved_data_bytes: 5 * 1024 * 1024,
            max_event_bytes: 16 * 1024,
        }
    }
}
impl Limits {
    pub fn bounded(mut self) -> Self {
        self.syntax = self.syntax.bounded();
        self.max_ir_depth = self.max_ir_depth.min(48);
        self.max_nodes = self.max_nodes.min(100_000);
        self.fuel_per_prop = self.fuel_per_prop.min(100_000);
        self.max_list_len = self.max_list_len.min(100_000);
        self.each_max_items = self.each_max_items.min(20_000);
        self.max_state_bytes = self.max_state_bytes.min(4 * 1024 * 1024);
        self.max_queries = self.max_queries.min(64);
        self.max_concurrent_requests = self.max_concurrent_requests.min(16);
        self.max_ref_resolutions = self.max_ref_resolutions.min(512);
        self.max_resolved_data_bytes = self.max_resolved_data_bytes.min(64 * 1024 * 1024);
        self.max_event_bytes = self.max_event_bytes.min(256 * 1024);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_semantic_limits_have_hard_ceilings() {
        let l = Limits {
            max_ir_depth: usize::MAX,
            max_nodes: usize::MAX,
            fuel_per_prop: usize::MAX,
            max_list_len: usize::MAX,
            each_max_items: usize::MAX,
            max_state_bytes: usize::MAX,
            max_queries: usize::MAX,
            max_concurrent_requests: usize::MAX,
            max_ref_resolutions: usize::MAX,
            max_resolved_data_bytes: usize::MAX,
            max_event_bytes: usize::MAX,
            ..Default::default()
        };
        let l = l.bounded();
        assert_eq!(
            (
                l.max_ir_depth,
                l.max_nodes,
                l.fuel_per_prop,
                l.max_list_len,
                l.each_max_items
            ),
            (48, 100_000, 100_000, 100_000, 20_000)
        );
        assert_eq!(
            (
                l.max_state_bytes,
                l.max_queries,
                l.max_concurrent_requests,
                l.max_ref_resolutions,
                l.max_resolved_data_bytes,
                l.max_event_bytes
            ),
            (4 * 1024 * 1024, 64, 16, 512, 64 * 1024 * 1024, 256 * 1024)
        );
    }
    #[test]
    fn zero_is_a_valid_lowered_limit() {
        let l: Limits = serde_json::from_str(r#"{"max_nodes":0,"max_string_bytes":0}"#).unwrap();
        assert_eq!(l.bounded().max_nodes, 0);
    }
}
