use std::path::Path;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use git2::{Repository, Sort};

use crate::{ChronicleError, Commit};

#[derive(Default)]
pub struct Filters {
    pub authors: Vec<String>,
    pub since: Option<NaiveDate>,
    pub branch: Option<String>,
    pub from_sha: Option<String>,
    pub to_sha: Option<String>,
}

/// Read commits from the repository at `path`, applying `filters`, and
/// optionally loading diffs.  Returns commits in chronological order
/// (oldest first).  Diffs are generated and converted to `String` one at a
/// time and never accumulated simultaneously.
pub fn ingest(
    path: &Path,
    filters: &Filters,
    include_diffs: bool,
) -> Result<Vec<Commit>, ChronicleError> {
    let repo = Repository::open(path)?;

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TIME)?;

    // Starting point: from_sha > branch > HEAD
    match &filters.from_sha {
        Some(sha) => revwalk.push(git2::Oid::from_str(sha)?)?,
        None => match &filters.branch {
            Some(branch) => revwalk.push_ref(&format!("refs/heads/{branch}"))?,
            None => revwalk.push_head()?,
        },
    }

    let to_oid = filters
        .to_sha
        .as_deref()
        .map(git2::Oid::from_str)
        .transpose()?;

    let mut commits = Vec::new();

    'walk: for oid_result in &mut revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;

        // Author filter — case-insensitive, matches name or email substring
        let author = commit.author();
        let name_lc = author.name().unwrap_or("").to_lowercase();
        let email_lc = author.email().unwrap_or("").to_lowercase();
        let author_matches = filters.authors.is_empty()
            || filters.authors.iter().any(|f| {
                let f = f.to_lowercase();
                name_lc.contains(&f) || email_lc.contains(&f)
            });

        // Timestamp
        let timestamp: DateTime<Utc> = Utc
            .timestamp_opt(commit.time().seconds(), 0)
            .single()
            .ok_or_else(|| {
                ChronicleError::GitError(git2::Error::from_str("invalid commit timestamp"))
            })?;

        // Date filter
        let since_matches = filters
            .since
            .map_or(true, |since| timestamp.date_naive() >= since);

        // Diff — generated and dropped immediately; never accumulated
        let diff = if include_diffs {
            Some(commit_diff(&repo, &commit)?)
        } else {
            None
        };

        let is_stop = to_oid.map_or(false, |to| oid == to);

        if author_matches && since_matches {
            commits.push(Commit {
                sha: oid.to_string(),
                author: format!(
                    "{} <{}>",
                    author.name().unwrap_or(""),
                    author.email().unwrap_or(""),
                ),
                timestamp,
                message: commit.message().unwrap_or("").trim_end().to_string(),
                diff,
            });
        }

        if is_stop {
            break 'walk;
        }
    }

    // revwalk yields newest-first; reverse for chronological output
    commits.reverse();
    Ok(commits)
}

/// Generate a unified diff for a single commit as a `String`.
/// The `git2::Diff` object is dropped before this function returns.
fn commit_diff(repo: &Repository, commit: &git2::Commit<'_>) -> Result<String, ChronicleError> {
    let new_tree = commit.tree()?;
    let old_tree = if commit.parent_count() > 0 {
        Some(commit.parent(0)?.tree()?)
    } else {
        None
    };

    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;

    let mut text = String::new();
    diff.print(git2::DiffFormat::Patch, |_, _, line| {
        text.push_str(&String::from_utf8_lossy(line.content()));
        true
    })?;

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ── test repo helpers ──────────────────────────────────────────────────

    struct TestRepo {
        _dir: tempfile::TempDir, // keep alive so the path stays valid
        repo: Repository,
    }

    impl TestRepo {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let repo = Repository::init(dir.path()).expect("init repo");
            Self { _dir: dir, repo }
        }

        fn path(&self) -> &Path {
            self._dir.path()
        }

        /// Add a commit with an explicit author and UNIX timestamp.
        fn add_commit(
            &self,
            message: &str,
            author_name: &str,
            author_email: &str,
            time_secs: i64,
        ) -> git2::Oid {
            let sig = git2::Signature::new(
                author_name,
                author_email,
                &git2::Time::new(time_secs, 0),
            )
            .expect("sig");

            // Write a file so the tree changes each commit
            let workdir = self.repo.workdir().expect("workdir");
            std::fs::write(workdir.join("content.txt"), message).expect("write file");

            let mut index = self.repo.index().expect("index");
            index.add_path(Path::new("content.txt")).expect("add path");
            index.write().expect("write index");
            let tree_id = index.write_tree().expect("write tree");
            let tree = self.repo.find_tree(tree_id).expect("find tree");

            let parents: Vec<git2::Commit<'_>> = match self.repo.head() {
                Ok(head) => {
                    let oid = head.target().expect("head target");
                    vec![self.repo.find_commit(oid).expect("parent commit")]
                }
                Err(_) => vec![],
            };
            let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();

            self.repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
                .expect("commit")
        }
    }

    // Known UNIX timestamps for test commits
    // 2024-01-01 00:00:00 UTC = 1704067200
    // 2024-02-01 00:00:00 UTC = 1706745600
    // 2024-03-01 00:00:00 UTC = 1709251200
    // 2024-04-01 00:00:00 UTC = 1711929600
    // 2024-05-01 00:00:00 UTC = 1714521600

    // ── tests ──────────────────────────────────────────────────────────────

    #[test]
    fn all_commits_no_filters() {
        let tr = TestRepo::new();
        let sha1 = tr.add_commit("first",  "Alice", "alice@test.com", 1704067200);
        let sha2 = tr.add_commit("second", "Bob",   "bob@test.com",   1706745600);
        let sha3 = tr.add_commit("third",  "Alice", "alice@test.com", 1709251200);

        let commits = ingest(tr.path(), &Filters::default(), false).expect("ingest");

        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].sha, sha1.to_string());
        assert_eq!(commits[1].sha, sha2.to_string());
        assert_eq!(commits[2].sha, sha3.to_string());
    }

    #[test]
    fn chronological_order() {
        let tr = TestRepo::new();
        tr.add_commit("first",  "Alice", "alice@test.com", 1704067200);
        tr.add_commit("second", "Alice", "alice@test.com", 1706745600);
        tr.add_commit("third",  "Alice", "alice@test.com", 1709251200);

        let commits = ingest(tr.path(), &Filters::default(), false).expect("ingest");

        assert_eq!(commits.len(), 3);
        assert!(commits[0].timestamp < commits[1].timestamp);
        assert!(commits[1].timestamp < commits[2].timestamp);
    }

    #[test]
    fn author_filter_by_name() {
        let tr = TestRepo::new();
        tr.add_commit("first",  "Alice", "alice@test.com", 1704067200);
        tr.add_commit("second", "Bob",   "bob@test.com",   1706745600);
        tr.add_commit("third",  "Alice", "alice@test.com", 1709251200);

        let filters = Filters {
            authors: vec!["alice".to_string()],
            ..Default::default()
        };
        let commits = ingest(tr.path(), &filters, false).expect("ingest");

        assert_eq!(commits.len(), 2);
        assert!(commits.iter().all(|c| c.author.to_lowercase().contains("alice")));
    }

    #[test]
    fn author_filter_by_email() {
        let tr = TestRepo::new();
        tr.add_commit("first",  "Alice", "alice@test.com", 1704067200);
        tr.add_commit("second", "Bob",   "bob@test.com",   1706745600);

        let filters = Filters {
            authors: vec!["bob@test.com".to_string()],
            ..Default::default()
        };
        let commits = ingest(tr.path(), &filters, false).expect("ingest");

        assert_eq!(commits.len(), 1);
        assert!(commits[0].author.contains("bob@test.com"));
    }

    #[test]
    fn author_filter_case_insensitive() {
        let tr = TestRepo::new();
        tr.add_commit("first",  "Alice", "ALICE@TEST.COM", 1704067200);
        tr.add_commit("second", "Bob",   "bob@test.com",   1706745600);

        let filters = Filters {
            authors: vec!["alice@test.com".to_string()],
            ..Default::default()
        };
        let commits = ingest(tr.path(), &filters, false).expect("ingest");

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].message, "first");
    }

    #[test]
    fn since_filter_excludes_older_commits() {
        let tr = TestRepo::new();
        tr.add_commit("jan", "Alice", "alice@test.com", 1704067200); // 2024-01-01
        tr.add_commit("feb", "Alice", "alice@test.com", 1706745600); // 2024-02-01
        tr.add_commit("mar", "Alice", "alice@test.com", 1709251200); // 2024-03-01

        let filters = Filters {
            since: Some(NaiveDate::from_ymd_opt(2024, 2, 1).expect("date")),
            ..Default::default()
        };
        let commits = ingest(tr.path(), &filters, false).expect("ingest");

        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].message, "feb");
        assert_eq!(commits[1].message, "mar");
    }

    #[test]
    fn since_filter_inclusive_on_exact_date() {
        let tr = TestRepo::new();
        tr.add_commit("jan", "Alice", "alice@test.com", 1704067200); // 2024-01-01
        tr.add_commit("feb", "Alice", "alice@test.com", 1706745600); // 2024-02-01

        let filters = Filters {
            since: Some(NaiveDate::from_ymd_opt(2024, 1, 1).expect("date")),
            ..Default::default()
        };
        let commits = ingest(tr.path(), &filters, false).expect("ingest");

        assert_eq!(commits.len(), 2);
    }

    #[test]
    fn sha_range_from_and_to() {
        let tr = TestRepo::new();
        let sha_a = tr.add_commit("A", "Alice", "alice@test.com", 1704067200); // oldest
        let sha_b = tr.add_commit("B", "Alice", "alice@test.com", 1706745600);
        let sha_c = tr.add_commit("C", "Alice", "alice@test.com", 1709251200);
        let sha_d = tr.add_commit("D", "Alice", "alice@test.com", 1711929600);
        let _sha_e = tr.add_commit("E", "Alice", "alice@test.com", 1714521600); // newest / HEAD

        // from_sha = D (revwalk start), to_sha = B (stop inclusive)
        // Walk goes D → C → B, reversed to B, C, D
        let filters = Filters {
            from_sha: Some(sha_d.to_string()),
            to_sha: Some(sha_b.to_string()),
            ..Default::default()
        };
        let commits = ingest(tr.path(), &filters, false).expect("ingest");

        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].sha, sha_b.to_string()); // oldest in range
        assert_eq!(commits[1].sha, sha_c.to_string());
        assert_eq!(commits[2].sha, sha_d.to_string()); // newest in range

        // Confirm A and E are absent
        let shas: Vec<&str> = commits.iter().map(|c| c.sha.as_str()).collect();
        assert!(!shas.contains(&sha_a.to_string().as_str()));
    }

    #[test]
    fn sha_range_to_only() {
        let tr = TestRepo::new();
        let sha_a = tr.add_commit("A", "Alice", "alice@test.com", 1704067200);
        tr.add_commit("B", "Alice", "alice@test.com", 1706745600);
        let sha_c = tr.add_commit("C", "Alice", "alice@test.com", 1709251200);

        // No from_sha — starts from HEAD (C); to_sha = A stops at A
        let filters = Filters {
            to_sha: Some(sha_a.to_string()),
            ..Default::default()
        };
        let commits = ingest(tr.path(), &filters, false).expect("ingest");

        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].sha, sha_a.to_string());
        assert_eq!(commits[2].sha, sha_c.to_string());
    }

    #[test]
    fn include_diffs_true_populates_diff() {
        let tr = TestRepo::new();
        tr.add_commit("add widget", "Alice", "alice@test.com", 1704067200);

        let commits = ingest(tr.path(), &Filters::default(), true).expect("ingest");

        assert_eq!(commits.len(), 1);
        let diff = commits[0].diff.as_ref().expect("diff should be Some");
        assert!(!diff.is_empty(), "diff should not be empty");
        // The diff should contain the content we wrote
        assert!(diff.contains("add widget"), "diff should contain commit content");
    }

    #[test]
    fn include_diffs_false_diff_is_none() {
        let tr = TestRepo::new();
        tr.add_commit("add widget", "Alice", "alice@test.com", 1704067200);

        let commits = ingest(tr.path(), &Filters::default(), false).expect("ingest");

        assert_eq!(commits.len(), 1);
        assert!(commits[0].diff.is_none());
    }

    #[test]
    fn invalid_repo_returns_git_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let result = ingest(dir.path(), &Filters::default(), false);
        assert!(matches!(result, Err(ChronicleError::GitError(_))));
    }
}
