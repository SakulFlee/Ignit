//! Classification and rate limiting for uncaptured WGPU errors.
//!
//! A single invalid resource can fail validation on every submit, which turns
//! one underlying problem into an unbounded log flood. The journal logs the
//! first occurrence of each distinct error with full details and then at most
//! one counted repeat per window.
//!
//! WGPU often reports the same failure twice per frame, once while recording
//! (`In a CommandEncoder`) and once at submit (`In Queue::submit`). Those two
//! descriptions share the innermost cause but differ in their context prefix,
//! so the fingerprint is taken from the end of the message. A small bounded
//! set of entries handles any remaining alternation without growing forever.

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

/// Maximum length of the stored summary, taken from the end of the message.
const SUMMARY_TAIL_LIMIT: usize = 240;
/// Upper bound on tracked distinct errors.
const MAX_TRACKED_ERRORS: usize = 8;

#[derive(Debug)]
struct Entry {
    kind: GpuErrorKind,
    fingerprint: u64,
    summary: String,
    last_logged: Instant,
    last_seen: Instant,
    suppressed: u64,
}

/// Rate limiter for repeated uncaptured errors.
#[derive(Debug)]
pub struct GpuErrorJournal {
    window: Duration,
    entries: Vec<Entry>,
}

impl Default for GpuErrorJournal {
    fn default() -> Self {
        Self::new(Duration::from_secs(5))
    }
}

impl GpuErrorJournal {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            entries: Vec::new(),
        }
    }

    /// Records one occurrence at `now` and decides whether it should be logged.
    pub fn record(&mut self, error: &wgpu::Error, now: Instant) -> GpuErrorDecision {
        let kind = GpuErrorKind::classify(error);
        let summary = summarize(describe(error));
        let fingerprint = fingerprint(kind, &summary);

        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.fingerprint == fingerprint)
        {
            entry.suppressed += 1;
            entry.last_seen = now;
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

        if self.entries.len() >= MAX_TRACKED_ERRORS
            && let Some((index, _)) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.last_seen)
        {
            self.entries.remove(index);
        }

        self.entries.push(Entry {
            kind,
            fingerprint,
            summary: summary.clone(),
            last_logged: now,
            last_seen: now,
            suppressed: 0,
        });
        GpuErrorDecision::First { kind, summary }
    }

    /// The most recently seen error, if any.
    pub fn snapshot(&self) -> Option<GpuErrorSnapshot> {
        self.entries
            .iter()
            .max_by_key(|entry| entry.last_seen)
            .map(|entry| GpuErrorSnapshot {
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

/// Collapses whitespace and keeps the innermost part of a WGPU message.
///
/// WGPU stacks the context in front of the actual failure:
///
/// ```text
/// Validation Error
///
/// Caused by:
///   In a CommandEncoder, label = '...'
///     In a set_pipeline command
///       <actual cause>
/// ```
///
/// The encoder and submit reports differ only in that context prefix, so the
/// tail is the stable part that identifies the resource and failure.
fn summarize(description: &str) -> String {
    let collapsed = description.split_whitespace().collect::<Vec<_>>().join(" ");
    let characters = collapsed.chars().collect::<Vec<_>>();
    let start = characters.len().saturating_sub(SUMMARY_TAIL_LIMIT);
    let tail = characters[start..].iter().collect::<String>();

    if start == 0 {
        return tail;
    }

    // The fixed-size tail may start in the middle of a word. Drop that partial
    // token so repeat lines begin at something readable.
    match tail.split_once(' ') {
        Some((_, rest)) => rest.to_string(),
        None => tail,
    }
}

fn fingerprint(kind: GpuErrorKind, summary: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    summary.hash(&mut hasher);
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

    fn long_cause() -> String {
        "In a set_pipeline command Render pipeline targets are incompatible with render pass \
         Incompatible color attachments at indices [0]: the RenderPass uses textures with one \
         format but the RenderPipeline with 'Ignit::GlarePipeline' label uses another format"
            .to_string()
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
    fn encoder_and_submit_reports_of_one_cause_share_a_fingerprint() {
        let cause = long_cause();
        let recording = validation(&format!(
            "Validation Error\n\nCaused by:\n  In a CommandEncoder, label = 'Ignit::Render::Encoder'\n    {cause}"
        ));
        let submit = validation(&format!(
            "Validation Error\n\nCaused by:\n  In Queue::submit\n    {cause}"
        ));
        let mut journal = GpuErrorJournal::new(Duration::from_secs(5));
        let start = Instant::now();

        assert!(matches!(
            journal.record(&recording, start),
            GpuErrorDecision::First { .. }
        ));
        assert_eq!(
            journal.record(&submit, start + Duration::from_millis(1)),
            GpuErrorDecision::Suppress
        );
        assert_eq!(journal.entries.len(), 1);
    }

    #[test]
    fn alternating_errors_are_each_suppressed() {
        let mut journal = GpuErrorJournal::new(Duration::from_secs(5));
        let a = validation("first distinct failure");
        let b = validation("second distinct failure");
        let start = Instant::now();

        assert!(matches!(
            journal.record(&a, start),
            GpuErrorDecision::First { .. }
        ));
        assert!(matches!(
            journal.record(&b, start),
            GpuErrorDecision::First { .. }
        ));
        assert_eq!(
            journal.record(&a, start + Duration::from_millis(1)),
            GpuErrorDecision::Suppress
        );
        assert_eq!(
            journal.record(&b, start + Duration::from_millis(1)),
            GpuErrorDecision::Suppress
        );
        assert_eq!(journal.entries.len(), 2);
    }

    #[test]
    fn the_journal_is_bounded() {
        let mut journal = GpuErrorJournal::new(Duration::from_secs(5));
        let start = Instant::now();

        for index in 0..=MAX_TRACKED_ERRORS {
            let error = validation(&format!("distinct failure number {index}"));
            journal.record(&error, start);
        }

        assert_eq!(journal.entries.len(), MAX_TRACKED_ERRORS);
    }

    #[test]
    fn summaries_keep_the_innermost_cause_and_are_bounded() {
        let cause = long_cause();
        let recording = format!(
            "Validation Error\n\nCaused by:\n  In a CommandEncoder, label = 'Ignit::Render::Encoder'\n    {cause}"
        );
        let submit = format!("Validation Error\n\nCaused by:\n  In Queue::submit\n    {cause}");

        assert_eq!(summarize(&recording), summarize(&submit));

        let normalized = summarize("  alpha\n\n beta\tgamma  ");
        assert_eq!(normalized, "alpha beta gamma");

        let long = "z".repeat(SUMMARY_TAIL_LIMIT * 2);
        assert_eq!(summarize(&long).chars().count(), SUMMARY_TAIL_LIMIT);
    }

    #[test]
    fn truncated_summaries_start_at_a_word_boundary() {
        let long = format!("{} visible tail", "p".repeat(SUMMARY_TAIL_LIMIT));
        assert_eq!(summarize(&long), "visible tail");
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
