use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub sha: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub diff: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

impl DateRange {
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        Self { from, to }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub text: String,
    pub commits: Vec<String>,
    pub authors: Vec<String>,
    pub date_range: DateRange,
    pub model: String,
    pub pass: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub pass: u32,
    pub commits: Vec<String>,
    pub authors: Vec<String>,
    pub date_range: DateRange,
    pub model: String,
    pub summary: String,
}

impl From<&Summary> for AuditEntry {
    fn from(s: &Summary) -> Self {
        Self {
            pass: s.pass,
            commits: s.commits.clone(),
            authors: s.authors.clone(),
            date_range: s.date_range.clone(),
            model: s.model.clone(),
            summary: s.text.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0)
            .single()
            .expect("valid date in test")
    }

    #[test]
    fn commit_roundtrip_with_diff() {
        let commit = Commit {
            sha: "abc123".to_string(),
            author: "Alice <alice@example.com>".to_string(),
            timestamp: utc(2024, 1, 15),
            message: "feat: add widget".to_string(),
            diff: Some("+fn widget() {}".to_string()),
        };
        let json = serde_json::to_string(&commit).expect("serialise");
        let back: Commit = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.sha, commit.sha);
        assert_eq!(back.diff, commit.diff);
    }

    #[test]
    fn commit_roundtrip_null_diff() {
        let commit = Commit {
            sha: "def456".to_string(),
            author: "Bob <bob@example.com>".to_string(),
            timestamp: utc(2024, 2, 1),
            message: "chore: tidy up".to_string(),
            diff: None,
        };
        let json = serde_json::to_string(&commit).expect("serialise");
        assert!(json.contains("\"diff\":null"), "diff should serialise as null");
        let back: Commit = serde_json::from_str(&json).expect("deserialise");
        assert!(back.diff.is_none());
    }

    #[test]
    fn date_range_new_and_roundtrip() {
        let from = utc(2024, 1, 1);
        let to = utc(2024, 3, 31);
        let dr = DateRange::new(from, to);
        assert_eq!(dr.from, from);
        assert_eq!(dr.to, to);

        let json = serde_json::to_string(&dr).expect("serialise");
        let back: DateRange = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.from, dr.from);
        assert_eq!(back.to, dr.to);
    }

    #[test]
    fn summary_roundtrip() {
        let summary = Summary {
            text: "Refactored auth module.".to_string(),
            commits: vec!["abc".to_string(), "def".to_string()],
            authors: vec!["Alice".to_string()],
            date_range: DateRange::new(utc(2024, 1, 1), utc(2024, 1, 31)),
            model: "llama3".to_string(),
            pass: 1,
        };
        let json = serde_json::to_string(&summary).expect("serialise");
        let back: Summary = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.text, summary.text);
        assert_eq!(back.pass, summary.pass);
        assert_eq!(back.commits, summary.commits);
    }

    #[test]
    fn audit_entry_roundtrip() {
        let entry = AuditEntry {
            pass: 2,
            commits: vec!["sha1".to_string()],
            authors: vec!["Carol".to_string()],
            date_range: DateRange::new(utc(2024, 4, 1), utc(2024, 4, 30)),
            model: "llama3".to_string(),
            summary: "Overhauled the pipeline.".to_string(),
        };
        let json = serde_json::to_string(&entry).expect("serialise");
        let back: AuditEntry = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.summary, entry.summary);
        assert_eq!(back.pass, entry.pass);
    }

    #[test]
    fn audit_entry_from_summary() {
        let summary = Summary {
            text: "Improved error handling.".to_string(),
            commits: vec!["aaa".to_string(), "bbb".to_string()],
            authors: vec!["Dave".to_string(), "Eve".to_string()],
            date_range: DateRange::new(utc(2024, 6, 1), utc(2024, 6, 15)),
            model: "mistral".to_string(),
            pass: 3,
        };
        let entry = AuditEntry::from(&summary);
        assert_eq!(entry.summary, summary.text);
        assert_eq!(entry.pass, summary.pass);
        assert_eq!(entry.commits, summary.commits);
        assert_eq!(entry.authors, summary.authors);
        assert_eq!(entry.model, summary.model);
        assert_eq!(entry.date_range.from, summary.date_range.from);
        assert_eq!(entry.date_range.to, summary.date_range.to);
    }
}
