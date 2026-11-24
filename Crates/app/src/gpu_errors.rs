//! Classification and rate limiting for uncaptured WGPU errors.
//!
//! A single invalid resource can fail validation on every submit, which turns
//! one underlying problem into an unbounded log flood. The journal logs the
//! first occurrence with full details and then at most one counted repeat per
//! window, while keeping the latest fingerprint available for the GPU health
//! state.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

/// Severity class of an uncaptured WGPU error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuErrorKind {
    OutOfMemory,
    Validation,
    Internal,
}

impl GpuErrorKind {
    pub fn classify(error: &wgpu::Error) -> Self {
        match error {
            wgpu::Error::OutOfMemory { .. } => Self::OutOfMemory,
            wgpu::Error::Validation { .. } => Self::Validation,
            wgpu::Error::Internal { .. } => Self::Internal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::OutOfMemory => "out of memory",
            Self::Validation => "validation",
            Self::Internal => "internal",
        }
    }
}

/// What the journal wants the caller to do with one occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuErrorDecision {
    /// First occurrence of an error; log it with full details.
    First { kind: GpuErrorKind, summary: String },
    /// The window elapsed for a repeated error; log it with a count.
    Repeat {
        kind: GpuErrorKind,
        summary: String,
        suppressed: u64,
    },
    /// The same error was already logged inside the current window.
    Suppress,
}

/// Snapshot of the most recent uncaptured error.
///
/// This is deliberately small and allocation-bounded so it can be read by the
/// GPU health state and debug UI without touching the journal lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuErrorSnapshot {
    pub kind: GpuErrorKind,
    pub fingerprint: u64,
    pub summary: String,
    /// Occurrences suppressed since the last log line.
    pub suppressed: u64,
}

/// Maximum length of the stored summary.
const SUMMARY_LIMIT: usize = 160;
/// Prefix used to fingerprint errors, ignoring volatile tails.
const FINGERPRINT_PREFIX_LIMIT: usize = 120;

#[derive(Debug)]
struct Entry {
    kind: GpuErrorKind,
    fingerprint: u64,
    summary: String,
    last_logged: Instant,
    suppressed: u64,
}

/// Rate limiter for repeated uncaptured errors.
#[derive(Debug)]
pub struct GpuErrorJournal {
    window: Duration,
    last: Option<Entry>,
}

impl Default for GpuErrorJournal {
    fn default() -> Self {
        Self::new(Duration::from_secs(5))
    }
}

impl GpuErrorJournal {
    pub fn new(window: Duration) -> Self {
        Self { window, last: None }
    }

    /// Records one occurrence at `now` and decides whether it should be logged.
    pub fn record(&mut self, error: &wgpu::Error, now: Instant) -> GpuErrorDecision {
        let kind = GpuErrorKind::classify(error);
        let summary = normalize(describe(error));
        let prefix = summary
            .chars()
            .take(FINGERPRINT_PREFIX_LIMIT)
            .collect::<String>();
        let fingerprint = fingerprint(kind, &prefix);

        if let Some(entry) = self.last.as_mut()
            && entry.fingerprint == fingerprint
        {
            entry.suppressed += 1;
            if now.saturating_duration_since(entry.last_logged) < self.window {
                return GpuErrorDecision::Suppress;
            }

            entry.last_logged = now;
            let suppressed = std::mem::take(&mut entry.suppressed);
            return GpuErrorDecision::Repeat {
                kind,
                summary: entry.summary.clone(),
                suppressed,
            };
        }

        self.last = Some(Entry {
            kind,
            fingerprint,
            summary: prefix.clone(),
            last_logged: now,
            suppressed: 0,
        });
        GpuErrorDecision::First {
            kind,
            summary: prefix,
        }
    }

    /// The most recent error, if any.
    pub fn snapshot(&self) -> Option<GpuErrorSnapshot> {
        self.last.as_ref().map(|entry| GpuErrorSnapshot {
            kind: entry.kind,
            fingerprint: entry.fingerprint,
            summary: entry.summary.clone(),
            suppressed: entry.suppressed,
        })
    }
}

fn describe(error: &wgpu::Error) -> &str {
    match error {
        wgpu::Error::OutOfMemory { .. } => "out of memory",
        wgpu::Error::Validation { description, .. } | wgpu::Error::Internal { description, .. } => {
            description
        }
    }
}

fn normalize(description: &str) -> String {
    let collapsed = description.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(SUMMARY_LIMIT).collect()
}

fn fingerprint(kind: GpuErrorKind, prefix: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    prefix.hash(&mut hasher);
    hasher.finish()
}

static JOURNAL: LazyLock<Mutex<GpuErrorJournal>> =
    LazyLock::new(|| Mutex::new(GpuErrorJournal::default()));

/// Records an uncaptured WGPU error and logs it if the journal allows it.
pub fn record_uncaptured(error: &wgpu::Error) {
    let decision = {
        let mut journal = JOURNAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        journal.record(error, Instant::now())
    };

    match decision {
        GpuErrorDecision::First { kind, .. } => {
            log::error!("wgpu uncaptured error ({}): {error:?}", kind.label());
        }
        GpuErrorDecision::Repeat {
            kind,
            summary,
            suppressed,
        } => {
            log::error!(
                "wgpu uncaptured error ({}) repeated {}x: {summary}",
                kind.label(),
                suppressed
            );
        }
        GpuErrorDecision::Suppress => {}
    }
}

/// Snapshot of the most recently recorded uncaptured error.
pub fn last_error() -> Option<GpuErrorSnapshot> {
    JOURNAL.lock().ok().and_then(|journal| journal.snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(std::io::Error::other("test source"))
    }

    fn validation(description: &str) -> wgpu::Error {
        wgpu::Error::Validation {
            source: source(),
            description: description.to_string(),
        }
    }

    #[test]
    fn errors_are_classified_by_kind() {
        assert_eq!(
            GpuErrorKind::classify(&wgpu::Error::OutOfMemory { source: source() }),
            GpuErrorKind::OutOfMemory
        );
        assert_eq!(
            GpuErrorKind::classify(&validation("bad pipeline")),
            GpuErrorKind::Validation
        );
        assert_eq!(
            GpuErrorKind::classify(&wgpu::Error::Internal {
                source: source(),
                description: "driver failure".to_string(),
            }),
            GpuErrorKind::Internal
        );
    }

    #[test]
    fn the_first_occurrence_logs_and_repeats_are_suppressed() {
        let mut journal = GpuErrorJournal::new(Duration::from_secs(5));
        let error = validation("RenderPipeline with 'ui' label is invalid");
        let start = Instant::now();

        assert!(matches!(
            journal.record(&error, start),
            GpuErrorDecision::First {
                kind: GpuErrorKind::Validation,
                ..
            }
        ));
        assert_eq!(
            journal.record(&error, start + Duration::from_secs(1)),
            GpuErrorDecision::Suppress
        );

        match journal.record(&error, start + Duration::from_secs(6)) {
            GpuErrorDecision::Repeat {
                kind,
                suppressed,
                summary,
            } => {
                assert_eq!(kind, GpuErrorKind::Validation);
                assert_eq!(suppressed, 2);
                assert!(summary.contains("'ui'"));
            }
            other => panic!("expected a counted repeat, got {other:?}"),
        }
    }

    #[test]
    fn distinct_errors_are_not_collapsed() {
        let mut journal = GpuErrorJournal::new(Duration::from_secs(5));
        let start = Instant::now();

        assert!(matches!(
            journal.record(&validation("pipeline 'a' is invalid"), start),
            GpuErrorDecision::First { .. }
        ));
        assert!(matches!(
            journal.record(&validation("pipeline 'b' is invalid"), start),
            GpuErrorDecision::First { .. }
        ));
        assert!(matches!(
            journal.record(&validation("pipeline 'a' is invalid"), start),
            GpuErrorDecision::First { .. }
        ));
    }

    #[test]
    fn volatile_tails_do_not_change_the_fingerprint() {
        let mut journal = GpuErrorJournal::new(Duration::from_secs(5));
        let start = Instant::now();
        let prefix = "x".repeat(FINGERPRINT_PREFIX_LIMIT);

        assert!(matches!(
            journal.record(&validation(&format!("{prefix} first tail")), start),
            GpuErrorDecision::First { .. }
        ));
        assert_eq!(
            journal.record(&validation(&format!("{prefix} second tail")), start),
            GpuErrorDecision::Suppress
        );
    }

    #[test]
    fn summaries_collapse_whitespace_and_are_bounded() {
        let normalized = normalize("  alpha\n\n beta\tgamma  ");
        assert_eq!(normalized, "alpha beta gamma");

        let long = "z".repeat(SUMMARY_LIMIT * 2);
        assert_eq!(normalize(&long).chars().count(), SUMMARY_LIMIT);
    }

    #[test]
    fn snapshot_tracks_the_latest_occurrence() {
        let mut journal = GpuErrorJournal::new(Duration::from_secs(5));
        let start = Instant::now();
        journal.record(&validation("first"), start);
        journal.record(&validation("first"), start + Duration::from_secs(1));

        let snapshot = journal.snapshot().expect("snapshot");
        assert_eq!(snapshot.kind, GpuErrorKind::Validation);
        assert_eq!(snapshot.suppressed, 1);
        assert_eq!(snapshot.summary, "first");
    }
}
