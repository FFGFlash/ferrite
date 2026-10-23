use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Number of power-of-two microsecond buckets: up to ~2^40 µs (12 days).
const BUCKETS: usize = 41;

/// Durations in power-of-two microsecond buckets. Percentiles are
/// approximate (within a factor of two, interpolated inside the bucket);
/// counts, means, and maxima are exact. Recording is O(1) and the size is
/// fixed, so histograms can record forever.
#[derive(Debug, Clone)]
pub struct Histogram {
  buckets: [u64; BUCKETS],
  count: u64,
  sum_us: u64,
  max_us: u64,
}

impl Default for Histogram {
  fn default() -> Self {
    Self {
      buckets: [0; BUCKETS],
      count: 0,
      sum_us: 0,
      max_us: 0,
    }
  }
}

/// Summary statistics, in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Summary {
  pub count: u64,
  pub mean_us: f64,
  pub p50_us: f64,
  pub p95_us: f64,
  pub p99_us: f64,
  pub max_us: u64,
}

impl Histogram {
  pub fn record(&mut self, duration: Duration) {
    let us = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
    let bucket = (u64::BITS - us.leading_zeros()) as usize;
    self.buckets[bucket.min(BUCKETS - 1)] += 1;
    self.count += 1;
    self.sum_us = self.sum_us.saturating_add(us);
    self.max_us = self.max_us.max(us);
  }

  pub fn summary(&self) -> Summary {
    if self.count == 0 {
      return Summary::default();
    }
    Summary {
      count: self.count,
      mean_us: self.sum_us as f64 / self.count as f64,
      p50_us: self.percentile(0.50),
      p95_us: self.percentile(0.95),
      p99_us: self.percentile(0.99),
      max_us: self.max_us,
    }
  }

  /// The approximate duration below which `fraction` of samples fall.
  fn percentile(&self, fraction: f64) -> f64 {
    let target = fraction * self.count as f64;
    let mut seen = 0.0;
    for (bucket, &count) in self.buckets.iter().enumerate() {
      if count == 0 {
        continue;
      }
      let next = seen + count as f64;
      if next >= target {
        // Bucket 0 holds 0 µs; bucket b holds [2^(b-1), 2^b).
        let (low, high) = if bucket == 0 {
          (0.0, 1.0)
        } else {
          (2f64.powi(bucket as i32 - 1), 2f64.powi(bucket as i32))
        };
        let within = (target - seen) / count as f64;
        return (low + (high - low) * within).min(self.max_us as f64);
      }
      seen = next;
    }
    self.max_us as f64
  }
}

/// Named histograms, safe to share between threads.
#[derive(Debug, Default)]
pub struct Metrics {
  histograms: Mutex<BTreeMap<&'static str, Histogram>>,
}

impl Metrics {
  pub fn record(&self, name: &'static str, duration: Duration) {
    self
      .histograms
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .entry(name)
      .or_default()
      .record(duration);
  }

  /// Starts timing `name`; the time is recorded when the timer is dropped.
  pub fn time(&self, name: &'static str) -> Timer<'_> {
    Timer {
      metrics: self,
      name,
      started: Instant::now(),
    }
  }

  pub fn snapshot(&self) -> Vec<(&'static str, Summary)> {
    self
      .histograms
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .iter()
      .map(|(&name, histogram)| (name, histogram.summary()))
      .collect()
  }
}

pub struct Timer<'a> {
  metrics: &'a Metrics,
  name: &'static str,
  started: Instant,
}

impl Drop for Timer<'_> {
  fn drop(&mut self) {
    self.metrics.record(self.name, self.started.elapsed());
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn summarizes_counts_means_and_maxima_exactly() {
    let mut histogram = Histogram::default();
    for us in [100, 200, 300, 400] {
      histogram.record(Duration::from_micros(us));
    }
    let summary = histogram.summary();
    assert_eq!(summary.count, 4);
    assert_eq!(summary.mean_us, 250.0);
    assert_eq!(summary.max_us, 400);
  }

  #[test]
  fn percentiles_are_within_a_factor_of_two() {
    let mut histogram = Histogram::default();
    for us in 1..=1000 {
      histogram.record(Duration::from_micros(us));
    }
    let summary = histogram.summary();
    assert!(
      (250.0..=1000.0).contains(&summary.p50_us),
      "{}",
      summary.p50_us
    );
    assert!(
      (475.0..=1000.0).contains(&summary.p95_us),
      "{}",
      summary.p95_us
    );
    assert!(summary.p99_us <= 1000.0);
  }

  #[test]
  fn empty_histograms_summarize_to_zero() {
    assert_eq!(Histogram::default().summary(), Summary::default());
  }

  #[test]
  fn timers_record_on_drop() {
    let metrics = Metrics::default();
    {
      let _timer = metrics.time("work");
    }
    metrics.record("work", Duration::from_millis(1));
    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].0, "work");
    assert_eq!(snapshot[0].1.count, 2);
  }
}
