use std::{collections::HashSet, path::PathBuf, sync::Arc};

use chrono::Utc;
use tokio::task::JoinSet;

use crate::{
    audit::AuditWriter,
    batcher,
    llm::LlmBackend,
    templates,
    types::{Commit, DateRange, Summary},
    ChronicleError,
};

pub struct ReduceConfig {
    pub group_size: usize,
    pub include_diffs: bool,
    pub template_dir: Option<PathBuf>,
    pub model: String,
}

/// Run the hierarchical reduce pipeline.
///
/// Pass 1 batches commits and calls the LLM concurrently for each group.
/// Subsequent passes batch the resulting summaries until a single summary
/// remains.  Every produced `Summary` is recorded to `audit` immediately.
pub async fn reduce(
    commits: Vec<Commit>,
    backend: Arc<dyn LlmBackend>,
    audit: &mut AuditWriter,
    config: &ReduceConfig,
) -> Result<String, ChronicleError> {
    if commits.is_empty() {
        return Err(ChronicleError::InvalidConfig(
            "no commits to summarise".to_string(),
        ));
    }

    // Pass 1: commits → summaries
    let batches = batcher::batch_commits(commits, config.group_size);
    let summaries = run_commit_pass(batches, Arc::clone(&backend), config, 1).await?;
    for s in &summaries {
        audit.record(s)?;
    }

    // Passes 2+: summaries → single summary
    fold_summaries(summaries, backend, audit, config, 2).await
}

// ── internal pass runners ──────────────────────────────────────────────────

async fn fold_summaries(
    mut summaries: Vec<Summary>,
    backend: Arc<dyn LlmBackend>,
    audit: &mut AuditWriter,
    config: &ReduceConfig,
    mut pass: u32,
) -> Result<String, ChronicleError> {
    while summaries.len() > 1 {
        let prev_len = summaries.len();
        let batches = batcher::batch_summaries(summaries, config.group_size);
        // is_final is true when this pass collapses everything into one group
        let is_final = batches.len() == 1;

        let new_summaries =
            run_summary_pass(batches, Arc::clone(&backend), config, pass, is_final).await?;

        debug_assert!(
            new_summaries.len() < prev_len,
            "each pass must strictly reduce summary count: was {prev_len}, now {}",
            new_summaries.len()
        );

        for s in &new_summaries {
            audit.record(s)?;
        }
        summaries = new_summaries;
        pass += 1;
    }

    summaries
        .into_iter()
        .next()
        .map(|s| s.text)
        .ok_or_else(|| ChronicleError::InvalidConfig("reduce produced no summaries".to_string()))
}

/// Render and submit all commit batches concurrently; return summaries in
/// batch order.
async fn run_commit_pass(
    batches: Vec<Vec<Commit>>,
    backend: Arc<dyn LlmBackend>,
    config: &ReduceConfig,
    pass: u32,
) -> Result<Vec<Summary>, ChronicleError> {
    let mut tasks: JoinSet<Result<(usize, Summary), ChronicleError>> = JoinSet::new();

    for (idx, batch) in batches.into_iter().enumerate() {
        let prompt =
            templates::render_batch(&batch, config.include_diffs, config.template_dir.as_deref())?;
        let shas: Vec<String> = batch.iter().map(|c| c.sha.clone()).collect();
        let authors = unique_authors(batch.iter().map(|c| c.author.as_str()));
        let date_range = commit_date_range(&batch);
        let backend = Arc::clone(&backend);
        let model = config.model.clone();

        tasks.spawn(async move {
            let text = backend.complete(&prompt).await?;
            Ok((idx, Summary { text, commits: shas, authors, date_range, model, pass }))
        });
    }

    collect_ordered(tasks).await
}

/// Render and submit all summary batches concurrently; return summaries in
/// batch order.
async fn run_summary_pass(
    batches: Vec<Vec<Summary>>,
    backend: Arc<dyn LlmBackend>,
    config: &ReduceConfig,
    pass: u32,
    is_final: bool,
) -> Result<Vec<Summary>, ChronicleError> {
    let mut tasks: JoinSet<Result<(usize, Summary), ChronicleError>> = JoinSet::new();

    for (idx, batch) in batches.into_iter().enumerate() {
        let prompt = templates::render_reduce(
            &batch,
            pass,
            is_final,
            config.template_dir.as_deref(),
        )?;
        let commits: Vec<String> =
            batch.iter().flat_map(|s| s.commits.iter().cloned()).collect();
        let authors = unique_authors(batch.iter().flat_map(|s| s.authors.iter().map(String::as_str)));
        let date_range = summary_date_range(&batch);
        let backend = Arc::clone(&backend);
        let model = config.model.clone();

        tasks.spawn(async move {
            let text = backend.complete(&prompt).await?;
            Ok((idx, Summary { text, commits, authors, date_range, model, pass }))
        });
    }

    collect_ordered(tasks).await
}

/// Drain a `JoinSet` that yields `(batch_index, T)`, abort on the first
/// error, and return items sorted by their original batch index.
async fn collect_ordered(
    mut tasks: JoinSet<Result<(usize, Summary), ChronicleError>>,
) -> Result<Vec<Summary>, ChronicleError> {
    let mut items: Vec<(usize, Summary)> = Vec::new();
    while let Some(join_result) = tasks.join_next().await {
        match join_result {
            Ok(Ok(item)) => items.push(item),
            Ok(Err(e)) => {
                tasks.abort_all();
                return Err(e);
            }
            Err(join_err) => {
                tasks.abort_all();
                return Err(ChronicleError::LlmFailure(format!(
                    "LLM task panicked: {join_err}"
                )));
            }
        }
    }
    items.sort_unstable_by_key(|(idx, _)| *idx);
    Ok(items.into_iter().map(|(_, s)| s).collect())
}

// ── metadata helpers ───────────────────────────────────────────────────────

fn unique_authors<'a>(iter: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for author in iter {
        if seen.insert(author.to_string()) {
            out.push(author.to_string());
        }
    }
    out
}

fn commit_date_range(commits: &[Commit]) -> DateRange {
    let min = commits
        .iter()
        .map(|c| c.timestamp)
        .min()
        .unwrap_or_else(Utc::now);
    let max = commits
        .iter()
        .map(|c| c.timestamp)
        .max()
        .unwrap_or_else(Utc::now);
    DateRange::new(min, max)
}

fn summary_date_range(summaries: &[Summary]) -> DateRange {
    let min = summaries
        .iter()
        .map(|s| s.date_range.from)
        .min()
        .unwrap_or_else(Utc::now);
    let max = summaries
        .iter()
        .map(|s| s.date_range.to)
        .max()
        .unwrap_or_else(Utc::now);
    DateRange::new(min, max)
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;

    use super::*;
    use crate::{audit::AuditWriter, llm::mock::MockBackend};

    // ── helpers ────────────────────────────────────────────────────────────

    fn ok(s: &str) -> Result<String, ChronicleError> {
        Ok(s.to_string())
    }

    fn make_commits(n: usize) -> Vec<Commit> {
        (0..n)
            .map(|i| Commit {
                sha: format!("{i:040x}"),
                author: format!("author{i} <a{i}@test.com>"),
                timestamp: Utc::now(),
                message: format!("commit {i}"),
                diff: None,
            })
            .collect()
    }

    fn config(group_size: usize) -> ReduceConfig {
        ReduceConfig {
            group_size,
            include_diffs: false,
            template_dir: None,
            model: "test-model".to_string(),
        }
    }

    // ── single-pass ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn single_batch_one_llm_call() {
        // 3 commits with group_size=20 → 1 batch → 1 LLM call → return directly
        let backend = Arc::new(MockBackend::new(vec![ok("the final narrative")]));
        let mut audit = AuditWriter::in_memory();

        let result = reduce(make_commits(3), backend.clone(), &mut audit, &config(20))
            .await
            .expect("reduce");

        assert_eq!(result, "the final narrative");
        assert_eq!(backend.call_count(), 1);
        assert_eq!(audit.entries().len(), 1);
        assert_eq!(audit.entries()[0].pass, 1);
    }

    // ── multi-pass ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn two_pass_reduction_correct_call_count() {
        // 4 commits, group_size=2:
        //   Pass 1: 2 batches → 2 calls → 2 summaries
        //   Pass 2: 1 batch (is_final=true) → 1 call → 1 summary
        //   Total: 3 LLM calls, 3 audit entries
        let backend = Arc::new(MockBackend::new(vec![
            ok("p1a"), ok("p1b"), // pass 1 (order of arrival is non-deterministic)
            ok("final"),          // pass 2
        ]));
        let mut audit = AuditWriter::in_memory();

        let result = reduce(make_commits(4), backend.clone(), &mut audit, &config(2))
            .await
            .expect("reduce");

        assert_eq!(result, "final");
        assert_eq!(backend.call_count(), 3);
        assert_eq!(audit.entries().len(), 3);
    }

    #[tokio::test]
    async fn three_pass_reduction() {
        // 8 commits, group_size=2:
        //   Pass 1: 4 batches → 4 calls → 4 summaries
        //   Pass 2: 2 batches (is_final=false) → 2 calls → 2 summaries
        //   Pass 3: 1 batch  (is_final=true)  → 1 call  → 1 summary
        //   Total: 7 calls, 7 audit entries
        let responses: Vec<_> = (0..6).map(|_| ok("intermediate")).chain([ok("done")]).collect();
        let backend = Arc::new(MockBackend::new(responses));
        let mut audit = AuditWriter::in_memory();

        let result = reduce(make_commits(8), backend.clone(), &mut audit, &config(2))
            .await
            .expect("reduce");

        assert_eq!(result, "done");
        assert_eq!(backend.call_count(), 7);
        assert_eq!(audit.entries().len(), 7);
    }

    // ── audit entries ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn audit_pass_numbers_assigned_correctly() {
        // 4 commits → 2 batches (pass 1) → 1 batch (pass 2)
        let backend = Arc::new(MockBackend::new(vec![
            ok("s1"), ok("s2"), ok("final"),
        ]));
        let mut audit = AuditWriter::in_memory();
        reduce(make_commits(4), backend, &mut audit, &config(2))
            .await
            .expect("reduce");

        let entries = audit.entries();
        assert_eq!(entries.len(), 3);
        // First 2 entries are from pass 1
        assert!(entries[..2].iter().all(|e| e.pass == 1));
        // Last entry is from pass 2
        assert_eq!(entries[2].pass, 2);
    }

    #[tokio::test]
    async fn audit_record_called_once_per_summary() {
        // Single-pass: 1 commit, 1 summary, 1 audit entry
        let backend = Arc::new(MockBackend::new(vec![ok("narrative")]));
        let mut audit = AuditWriter::in_memory();
        reduce(make_commits(1), backend, &mut audit, &config(20))
            .await
            .expect("reduce");
        assert_eq!(audit.entries().len(), 1);
    }

    // ── is_final ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn is_final_only_on_last_pass() {
        // 4 commits, group_size=2 → 2-pass reduction
        // Pass 1: render_batch prompts (no is_final concept)
        // Pass 2: render_reduce(is_final=true) → prompt contains "narrative"
        // All pass-1 prompts come from render_batch which never contains "narrative".
        let backend = Arc::new(MockBackend::new(vec![
            ok("s1"), ok("s2"), ok("final"),
        ]));
        let mut audit = AuditWriter::in_memory();
        reduce(make_commits(4), backend.clone(), &mut audit, &config(2))
            .await
            .expect("reduce");

        let prompts = backend.recorded_prompts();
        assert_eq!(prompts.len(), 3);

        // Pass-1 prompts are batch prompts — they mention "summarising"
        // but NOT "narrative" (that's only in reduce.tera with is_final=true)
        for p in &prompts[..2] {
            assert!(
                !p.contains("narrative"),
                "pass-1 prompt should not request a narrative:\n{p}"
            );
        }
        // Pass-2 prompt uses reduce.tera with is_final=true
        assert!(
            prompts[2].contains("narrative"),
            "final-pass prompt must request a narrative:\n{}",
            prompts[2]
        );
    }

    #[tokio::test]
    async fn intermediate_pass_prompt_says_intermediate() {
        // 8 commits, group_size=2 → 3 passes
        // Pass 2 has 2 batches and is NOT final → should say "intermediate"
        // Pass 3 has 1 batch and IS final → should say "narrative"
        let responses: Vec<_> =
            (0..6).map(|_| ok("mid")).chain([ok("done")]).collect();
        let backend = Arc::new(MockBackend::new(responses));
        let mut audit = AuditWriter::in_memory();
        reduce(make_commits(8), backend.clone(), &mut audit, &config(2))
            .await
            .expect("reduce");

        let prompts = backend.recorded_prompts();
        assert_eq!(prompts.len(), 7);

        // prompts[0..4]: pass-1 (batch prompts, no intermediate/narrative)
        // prompts[4..6]: pass-2 (reduce, is_final=false → "intermediate")
        // prompts[6]:    pass-3 (reduce, is_final=true  → "narrative")
        for p in &prompts[4..6] {
            assert!(
                p.contains("intermediate"),
                "pass-2 prompt must say 'intermediate':\n{p}"
            );
        }
        assert!(
            prompts[6].contains("narrative"),
            "pass-3 prompt must say 'narrative':\n{}",
            prompts[6]
        );
    }

    // ── error cases ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn empty_commits_returns_error() {
        let backend = Arc::new(MockBackend::new(vec![]));
        let mut audit = AuditWriter::in_memory();
        let err = reduce(vec![], backend, &mut audit, &config(20))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no commits"), "got: {err}");
    }

    #[tokio::test]
    async fn llm_failure_propagates() {
        let backend = Arc::new(MockBackend::new(vec![Err(ChronicleError::LlmFailure(
            "backend down".to_string(),
        ))]));
        let mut audit = AuditWriter::in_memory();
        let err = reduce(make_commits(1), backend, &mut audit, &config(20))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("backend down"), "got: {err}");
    }
}
