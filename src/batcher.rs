use crate::{Commit, Summary};

/// Split `commits` into consecutive groups of at most `group_size`.
/// The final group may be smaller.  Returns an empty `Vec` for empty input.
/// `group_size` >= 2 is enforced by CLI validation; asserted in debug builds.
pub fn batch_commits(commits: Vec<Commit>, group_size: usize) -> Vec<Vec<Commit>> {
    debug_assert!(group_size >= 2);
    chunk(commits, group_size)
}

/// Split `summaries` into consecutive groups of at most `group_size`.
/// The final group may be smaller.  Returns an empty `Vec` for empty input.
/// `group_size` >= 2 is enforced by CLI validation; asserted in debug builds.
pub fn batch_summaries(summaries: Vec<Summary>, group_size: usize) -> Vec<Vec<Summary>> {
    debug_assert!(group_size >= 2);
    chunk(summaries, group_size)
}

fn chunk<T>(mut items: Vec<T>, group_size: usize) -> Vec<Vec<T>> {
    if items.is_empty() {
        return Vec::new();
    }
    let mut batches = Vec::with_capacity(items.len().div_ceil(group_size));
    while items.len() > group_size {
        let tail = items.split_off(group_size);
        batches.push(items);
        items = tail;
    }
    batches.push(items);
    batches
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_commits(n: usize) -> Vec<Commit> {
        (0..n)
            .map(|i| Commit {
                sha: format!("{i:040}"),
                author: "Alice <alice@test.com>".to_string(),
                timestamp: Utc::now(),
                message: format!("commit {i}"),
                diff: None,
            })
            .collect()
    }

    fn make_summaries(n: usize) -> Vec<Summary> {
        use crate::{DateRange, Summary};
        (0..n)
            .map(|i| Summary {
                text: format!("summary {i}"),
                commits: vec![],
                authors: vec![],
                date_range: DateRange::new(Utc::now(), Utc::now()),
                model: "test".to_string(),
                pass: 1,
                duration_ms: 0,
            })
            .collect()
    }

    // ── batch_commits ──────────────────────────────────────────────────────

    #[test]
    fn commits_empty_input() {
        assert!(batch_commits(vec![], 3).is_empty());
    }

    #[test]
    fn commits_less_than_group_size() {
        let batches = batch_commits(make_commits(2), 5);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 2);
    }

    #[test]
    fn commits_exactly_group_size() {
        let batches = batch_commits(make_commits(4), 4);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 4);
    }

    #[test]
    fn commits_greater_than_group_size_uneven() {
        let batches = batch_commits(make_commits(7), 3);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 3);
        assert_eq!(batches[1].len(), 3);
        assert_eq!(batches[2].len(), 1); // remainder
    }

    #[test]
    fn commits_exactly_divisible() {
        let batches = batch_commits(make_commits(6), 3);
        assert_eq!(batches.len(), 2);
        assert!(batches.iter().all(|b| b.len() == 3));
    }

    #[test]
    fn commits_order_preserved() {
        let commits = make_commits(5);
        let expected_shas: Vec<String> = commits.iter().map(|c| c.sha.clone()).collect();
        let batches = batch_commits(commits, 3);
        let got_shas: Vec<String> = batches.into_iter().flatten().map(|c| c.sha).collect();
        assert_eq!(got_shas, expected_shas);
    }

    // ── batch_summaries ────────────────────────────────────────────────────

    #[test]
    fn summaries_empty_input() {
        assert!(batch_summaries(vec![], 3).is_empty());
    }

    #[test]
    fn summaries_less_than_group_size() {
        let batches = batch_summaries(make_summaries(2), 5);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 2);
    }

    #[test]
    fn summaries_exactly_group_size() {
        let batches = batch_summaries(make_summaries(4), 4);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 4);
    }

    #[test]
    fn summaries_greater_than_group_size_uneven() {
        let batches = batch_summaries(make_summaries(7), 3);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), 3);
        assert_eq!(batches[1].len(), 3);
        assert_eq!(batches[2].len(), 1);
    }

    #[test]
    fn summaries_exactly_divisible() {
        let batches = batch_summaries(make_summaries(6), 3);
        assert_eq!(batches.len(), 2);
        assert!(batches.iter().all(|b| b.len() == 3));
    }

    #[test]
    fn summaries_order_preserved() {
        let summaries = make_summaries(5);
        let expected: Vec<String> = summaries.iter().map(|s| s.text.clone()).collect();
        let batches = batch_summaries(summaries, 3);
        let got: Vec<String> = batches.into_iter().flatten().map(|s| s.text).collect();
        assert_eq!(got, expected);
    }
}
