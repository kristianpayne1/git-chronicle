use std::{
    io::{BufWriter, Write},
    path::Path,
};

use crate::{
    types::{AuditEntry, Summary},
    ChronicleError,
};

/// Accumulates `AuditEntry` values and, when a path is configured, writes
/// each entry as a JSON line immediately — so partial results survive an
/// interrupted run.
pub struct AuditWriter {
    entries: Vec<AuditEntry>,
    file: Option<BufWriter<std::fs::File>>,
}

impl AuditWriter {
    /// Create an `AuditWriter` that writes JSON lines to `path`.
    /// Pass `None` to collect entries in memory only (useful in tests and
    /// when `--output` is not set).
    pub fn new(path: Option<&Path>) -> Result<Self, ChronicleError> {
        let file = path
            .map(|p| {
                let f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .map_err(ChronicleError::IoError)?;
                Ok::<_, ChronicleError>(BufWriter::new(f))
            })
            .transpose()?;
        Ok(Self { entries: Vec::new(), file })
    }

    /// In-memory only — no file I/O.  Convenience constructor for tests.
    pub fn in_memory() -> Self {
        Self { entries: Vec::new(), file: None }
    }

    /// Convert `summary` into an `AuditEntry`, append it to the in-memory
    /// list, and flush it to disk immediately if a file is configured.
    pub fn record(&mut self, summary: &Summary) -> Result<(), ChronicleError> {
        let entry = AuditEntry::from(summary);
        if let Some(ref mut writer) = self.file {
            let line = serde_json::to_string(&entry)?;
            writeln!(writer, "{line}")?;
            writer.flush()?;
        }
        self.entries.push(entry);
        Ok(())
    }

    /// All entries recorded so far, in recording order.
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::types::DateRange;

    fn sample_summary(text: &str, pass: u32) -> Summary {
        Summary {
            text: text.to_string(),
            commits: vec!["abc123".to_string()],
            authors: vec!["Alice".to_string()],
            date_range: DateRange::new(Utc::now(), Utc::now()),
            model: "test-model".to_string(),
            pass,
            duration_ms: 0,
        }
    }

    #[test]
    fn record_accumulates_entries() {
        let mut writer = AuditWriter::in_memory();
        writer.record(&sample_summary("first", 1)).expect("record");
        writer.record(&sample_summary("second", 2)).expect("record");
        assert_eq!(writer.entries().len(), 2);
        assert_eq!(writer.entries()[0].summary, "first");
        assert_eq!(writer.entries()[1].summary, "second");
    }

    #[test]
    fn record_maps_fields_correctly() {
        let summary = sample_summary("the narrative", 3);
        let mut writer = AuditWriter::in_memory();
        writer.record(&summary).expect("record");
        let entry = &writer.entries()[0];
        assert_eq!(entry.summary, summary.text);
        assert_eq!(entry.pass, summary.pass);
        assert_eq!(entry.model, summary.model);
    }

    #[test]
    fn file_writer_produces_valid_json_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.jsonl");
        {
            let mut writer = AuditWriter::new(Some(&path)).expect("new");
            writer.record(&sample_summary("pass1", 1)).expect("record");
            writer.record(&sample_summary("pass2", 2)).expect("record");
        }
        let content = std::fs::read_to_string(&path).expect("read");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        // Each line must deserialise as a valid AuditEntry
        for line in lines {
            serde_json::from_str::<AuditEntry>(line).expect("valid json");
        }
    }
}
