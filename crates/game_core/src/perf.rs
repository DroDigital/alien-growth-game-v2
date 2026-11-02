//! Frame-time statistics: average fps, 1% lows, spikes.
//!
//! Used by the benchmark mode, the perf overlay and the launcher, which reads
//! [`BenchmarkReport`] JSON written by `game --benchmark`.

use serde::{Deserialize, Serialize};

/// Frame-time spike threshold from the performance targets (ms).
pub const SPIKE_THRESHOLD_MS: f64 = 50.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameStats {
    pub frames: usize,
    pub avg_fps: f64,
    /// Average fps over the slowest 1% of frames.
    pub low_1pct_fps: f64,
    pub avg_frame_ms: f64,
    pub max_frame_ms: f64,
    /// Frames slower than [`SPIKE_THRESHOLD_MS`].
    pub spikes_over_50ms: usize,
}

impl FrameStats {
    /// Computes stats from frame times in milliseconds. Returns `None` for an
    /// empty slice.
    pub fn from_frame_times_ms(times: &[f64]) -> Option<Self> {
        if times.is_empty() {
            return None;
        }
        let total: f64 = times.iter().sum();
        let avg_ms = total / times.len() as f64;
        let mut sorted = times.to_vec();
        sorted.sort_by(|a, b| b.total_cmp(a)); // slowest first
        let n_low = (times.len() / 100).max(1);
        let low_avg_ms = sorted[..n_low].iter().sum::<f64>() / n_low as f64;
        Some(Self {
            frames: times.len(),
            avg_fps: 1000.0 / avg_ms,
            low_1pct_fps: 1000.0 / low_avg_ms,
            avg_frame_ms: avg_ms,
            max_frame_ms: sorted[0],
            spikes_over_50ms: times.iter().filter(|&&t| t > SPIKE_THRESHOLD_MS).count(),
        })
    }
}

/// Output of `game --benchmark`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub game_version: String,
    pub scene: String,
    pub preset: String,
    pub output_resolution: (u32, u32),
    pub render_resolution: (u32, u32),
    pub adapter: String,
    pub backend: String,
    pub duration_secs: f64,
    /// Frames ignored at the start (shader compilation, asset loading).
    pub warmup_frames: usize,
    pub stats: FrameStats,
    /// Process resident memory at the end of the run, if available (MiB).
    pub memory_mib: Option<f64>,
    /// The preset the launcher should recommend based on this run.
    pub recommended_preset: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_basic() {
        // 99 frames at 10 ms, one at 60 ms.
        let mut t = vec![10.0; 99];
        t.push(60.0);
        let s = FrameStats::from_frame_times_ms(&t).unwrap();
        assert_eq!(s.frames, 100);
        assert!((s.avg_frame_ms - 10.5).abs() < 1e-9);
        assert!((s.low_1pct_fps - 1000.0 / 60.0).abs() < 1e-9);
        assert_eq!(s.max_frame_ms, 60.0);
        assert_eq!(s.spikes_over_50ms, 1);
    }

    #[test]
    fn stats_empty() {
        assert!(FrameStats::from_frame_times_ms(&[]).is_none());
    }
}
