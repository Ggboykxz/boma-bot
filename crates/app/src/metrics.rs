//! Prometheus text exposition of what this process knows about itself.
//!
//! Deliberately hand-rolled: the counter surface fits in one screen, and a registry dependency
//! would buy exposition the deployment never asked for at the price of every future upgrade.
//! Counters only — no gauges, no histograms: run timings land here as millisecond-sum counters,
//! matching the reference's named-field run logs, not as geometry nobody queries.
//!
//! A poisoned lock is a harness bug, never a 500: increments are dropped, so a request never
//! fails because telemetry tripped.

use std::{collections::BTreeMap, fmt::Write as _, sync::Mutex};

/// The counters of one process.
#[derive(Debug, Default)]
pub struct Metrics {
    inner: Mutex<BTreeMap<String, u64>>,
}

impl Metrics {
    /// An empty registry; a deployment starts from zero on each boot, which is what a counter
    /// family scraped by Prometheus wants.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances one counter by 1.
    ///
    /// Labels are escaped minimally and keyed with label names in sorted order, so `{a, b}` and
    /// `{b, a}` are the same counter.
    pub fn increment(&self, name: &str, labels: &[(&str, &str)]) {
        self.add(name, labels, 1);
    }

    /// Advances one counter by `n`; used for sums such as total milliseconds spent on replies.
    pub fn add(&self, name: &str, labels: &[(&str, &str)], n: u64) {
        let key = key_of(name, labels);
        if let Ok(mut guard) = self.inner.lock() {
            *guard.entry(key).or_default() += n;
        }
    }

    /// The body of `GET /metrics`: one `<key> <value>` line per counter, sorted by key.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Ok(guard) = self.inner.lock() {
            for (key, value) in guard.iter() {
                let _ = writeln!(out, "{key} {value}");
            }
        }
        out
    }
}

fn key_of(name: &str, labels: &[(&str, &str)]) -> String {
    let mut key = String::from(name);
    if labels.is_empty() {
        return key;
    }
    let mut sorted: Vec<(&str, &str)> = labels.to_vec();
    sorted.sort_by(|left, right| left.0.cmp(right.0));
    key.push('{');
    for (position, (label_name, value)) in sorted.into_iter().enumerate() {
        if position > 0 {
            key.push(',');
        }
        let _ = write!(key, "{label_name}=\"{}\"", escape(value));
    }
    key.push('}');
    key
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::Metrics;

    #[test]
    fn req_054_counters_aggregate_by_name_and_sorted_labels() {
        let metrics = Metrics::new();
        metrics.increment("boma_runs_started_total", &[("trigger", "user")]);
        metrics.increment("boma_runs_started_total", &[("trigger", "user")]);
        metrics.increment("boma_runs_started_total", &[("trigger", "routine")]);
        metrics.add("boma_run_first_response_ms_total", &[], 41);
        metrics.add("boma_run_first_response_ms_total", &[], 9);
        let out = metrics.render();
        assert!(
            out.contains("boma_runs_started_total{trigger=\"user\"} 2\n"),
            "{out}"
        );
        assert!(
            out.contains("boma_runs_started_total{trigger=\"routine\"} 1\n"),
            "{out}"
        );
        assert!(
            out.contains("boma_run_first_response_ms_total 50\n"),
            "{out}"
        );
    }

    #[test]
    fn req_054_render_is_sorted_and_stable() {
        let metrics = Metrics::new();
        metrics.increment("b_series", &[]);
        metrics.increment("a_series", &[]);
        let out = metrics.render();
        let lines: Vec<&str> = out.lines().collect();
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        assert_eq!(lines, sorted);
        assert!(out.contains("a_series 1\n"), "{out}");
        assert!(out.contains("b_series 1\n"), "{out}");
    }
}
