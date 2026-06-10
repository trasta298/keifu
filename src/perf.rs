//! Lightweight performance counters
//!
//! Aggregates are recorded by the main loop and app operations and can be
//! inspected at runtime via the debug server's `{"cmd":"perf"}` command.
//! Operations slower than the threshold are also emitted as tracing events.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

/// Operations at or above this duration enter the recent-slow log
const SLOW_THRESHOLD: Duration = Duration::from_millis(10);
const SLOW_LOG_CAP: usize = 50;

#[derive(Debug, Default, Clone, Copy)]
pub struct Aggregate {
    pub count: u64,
    pub total: Duration,
    pub max: Duration,
    pub last: Duration,
}

impl Aggregate {
    fn record(&mut self, duration: Duration) {
        self.count += 1;
        self.total += duration;
        self.last = duration;
        if duration > self.max {
            self.max = duration;
        }
    }

    pub fn avg(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            self.total / self.count as u32
        }
    }
}

#[derive(Debug, Default)]
pub struct PerfStats {
    ops: HashMap<&'static str, Aggregate>,
    slow_log: VecDeque<(&'static str, Duration)>,
}

impl PerfStats {
    pub fn record(&mut self, name: &'static str, duration: Duration) {
        self.ops.entry(name).or_default().record(duration);
        if duration >= SLOW_THRESHOLD {
            tracing::debug!(
                op = name,
                ms = duration.as_millis() as u64,
                "slow operation"
            );
            if self.slow_log.len() == SLOW_LOG_CAP {
                self.slow_log.pop_front();
            }
            self.slow_log.push_back((name, duration));
        }
    }

    pub fn ops(&self) -> impl Iterator<Item = (&'static str, &Aggregate)> {
        self.ops.iter().map(|(name, agg)| (*name, agg))
    }

    /// Most recent slow operations, oldest first
    pub fn slow_log(&self) -> impl Iterator<Item = (&'static str, Duration)> + '_ {
        self.slow_log.iter().copied()
    }

    /// Emit an aggregate summary to the log (called on exit)
    pub fn log_summary(&self) {
        let ms = |d: Duration| (d.as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
        let mut ops: Vec<_> = self.ops().collect();
        ops.sort_by_key(|(name, _)| *name);
        for (name, agg) in ops {
            tracing::info!(
                op = name,
                count = agg.count,
                avg_ms = ms(agg.avg()),
                max_ms = ms(agg.max),
                "perf summary"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_aggregates_and_slow_log() {
        let mut perf = PerfStats::default();
        perf.record("fast", Duration::from_millis(1));
        perf.record("slow", Duration::from_millis(20));
        perf.record("slow", Duration::from_millis(40));

        let slow = perf.ops().find(|(n, _)| *n == "slow").unwrap().1;
        assert_eq!(slow.count, 2);
        assert_eq!(slow.max, Duration::from_millis(40));
        assert_eq!(slow.avg(), Duration::from_millis(30));
        assert_eq!(perf.slow_log().count(), 2);
        assert_eq!(perf.ops().find(|(n, _)| *n == "fast").unwrap().1.count, 1);
    }
}
