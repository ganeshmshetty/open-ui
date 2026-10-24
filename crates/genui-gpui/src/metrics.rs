//! Renderer-side measurements (SPEC §10.6). Numbers are reported, not gating.

use std::collections::BTreeMap;
use std::time::Duration;

pub const MAX_PATCH_LOG: usize = 512;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatchSummary {
    pub epoch: u32,
    pub seq: u64,
    pub ops: usize,
    pub applied: bool,
    pub discarded: bool,
    pub needs_snapshot: bool,
    pub errors: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub frames: u64,
    pub queue_drains: u64,
    pub batches_applied: u64,
    pub batches_enqueued: u64,
    pub ops_applied: u64,
    pub nodes_inserted: u64,
    pub nodes_removed: u64,
    pub node_renders: BTreeMap<String, u64>,
    pub chart_renders: u64,
    pub image_renders: u64,
    pub node_render_ns: u64,
    pub table_parse_ns: u64,
    pub snapshot_requests: u64,
    pub apply_samples: Vec<Duration>,
    pub render_samples: Vec<Duration>,
    /// Node-element work accrued before each `UiView::render` boundary.
    pub frame_app_samples: Vec<Duration>,
    pub frame_app_baseline_ns: u64,
    pub patch_log: Vec<PatchSummary>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timing {
    pub count: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub frames: u64,
    pub queue_drains: u64,
    pub batches_applied: u64,
    pub batches_enqueued: u64,
    pub ops_applied: u64,
    pub nodes_inserted: u64,
    pub nodes_removed: u64,
    pub chart_renders: u64,
    pub image_renders: u64,
    pub node_render_ns: u64,
    pub table_parse_ns: u64,
    pub snapshot_requests: u64,
    pub apply: Timing,
    pub render: Timing,
    pub frame_app: Timing,
    pub patch_log_entries: usize,
}

impl Metrics {
    pub fn record_patch(&mut self, summary: PatchSummary) {
        if self.patch_log.len() == MAX_PATCH_LOG {
            self.patch_log.remove(0);
        }
        self.patch_log.push(summary);
    }

    pub fn record_apply(&mut self, elapsed: Duration) {
        self.apply_samples.push(elapsed);
    }

    pub fn record_render(&mut self, elapsed: Duration) {
        self.frames += 1;
        self.render_samples.push(elapsed);
    }

    pub fn summary(&self) -> Summary {
        Summary {
            frames: self.frames,
            queue_drains: self.queue_drains,
            batches_applied: self.batches_applied,
            batches_enqueued: self.batches_enqueued,
            ops_applied: self.ops_applied,
            nodes_inserted: self.nodes_inserted,
            nodes_removed: self.nodes_removed,
            chart_renders: self.chart_renders,
            image_renders: self.image_renders,
            node_render_ns: self.node_render_ns,
            table_parse_ns: self.table_parse_ns,
            snapshot_requests: self.snapshot_requests,
            apply: timing(&self.apply_samples),
            render: timing(&self.render_samples),
            frame_app: timing(&self.frame_app_samples),
            patch_log_entries: self.patch_log.len(),
        }
    }
}

fn timing(samples: &[Duration]) -> Timing {
    if samples.is_empty() {
        return Timing::default();
    }
    let mut millis: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    millis.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Timing {
        count: millis.len(),
        p50_ms: percentile(&millis, 0.50),
        p95_ms: percentile(&millis, 0.95),
        p99_ms: percentile(&millis, 0.99),
        max_ms: millis.last().copied().unwrap_or(0.0),
    }
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() as f64 - 1.0) * fraction.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_nearest_rank_and_monotonic() {
        let samples: Vec<Duration> = (1..=100)
            .map(|value| Duration::from_micros(value * 10))
            .collect();
        let timing = timing(&samples);
        assert_eq!(timing.count, 100);
        assert_eq!(timing.p50_ms, 0.51);
        assert_eq!(timing.p95_ms, 0.95);
        assert_eq!(timing.p99_ms, 0.99);
        assert_eq!(timing.max_ms, 1.0);
    }

    #[test]
    fn patch_log_is_bounded_and_keeps_the_latest_entries() {
        let mut metrics = Metrics::default();
        for seq in 0..(MAX_PATCH_LOG as u64 + 5) {
            metrics.record_patch(PatchSummary {
                epoch: 1,
                seq,
                ops: 1,
                applied: true,
                ..Default::default()
            });
        }
        assert_eq!(metrics.patch_log.len(), MAX_PATCH_LOG);
        assert_eq!(metrics.patch_log.first().map(|entry| entry.seq), Some(5));
        assert_eq!(metrics.summary().patch_log_entries, MAX_PATCH_LOG);
    }
}
