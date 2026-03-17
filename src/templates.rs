use std::path::Path;

use tera::{Context, Tera};

use crate::{ChronicleError, Commit, Summary};

const BUILTIN_BATCH: &str = include_str!("../templates/batch.tera");
const BUILTIN_REDUCE: &str = include_str!("../templates/reduce.tera");

/// Render the batch prompt for a slice of commits.
///
/// `include_diffs` gates whether diff content is emitted in the prompt even
/// when `Commit.diff` is `Some`.  `template_dir` overrides the built-in
/// `batch.tera` if a file named `batch.tera` exists in that directory.
pub fn render_batch(
    commits: &[Commit],
    include_diffs: bool,
    template_dir: Option<&Path>,
) -> Result<String, ChronicleError> {
    let src = resolve("batch.tera", BUILTIN_BATCH, template_dir)?;
    let mut tera = Tera::default();
    tera.add_raw_template("batch.tera", &src)?;

    let mut ctx = Context::new();
    ctx.insert("commits", commits);
    ctx.insert("include_diffs", &include_diffs);

    Ok(tera.render("batch.tera", &ctx)?)
}

/// Render the reduce prompt for a slice of summaries.
///
/// `pass` is the current reduction pass number (1-indexed).  `is_final`
/// changes the instruction tone: final pass requests a complete narrative;
/// intermediate passes request a dense intermediate summary.  `template_dir`
/// overrides the built-in `reduce.tera` if a file named `reduce.tera`
/// exists in that directory.
pub fn render_reduce(
    summaries: &[Summary],
    pass: u32,
    is_final: bool,
    template_dir: Option<&Path>,
) -> Result<String, ChronicleError> {
    let src = resolve("reduce.tera", BUILTIN_REDUCE, template_dir)?;
    let mut tera = Tera::default();
    tera.add_raw_template("reduce.tera", &src)?;

    let mut ctx = Context::new();
    ctx.insert("summaries", summaries);
    ctx.insert("pass", &pass);
    ctx.insert("is_final", &is_final);

    Ok(tera.render("reduce.tera", &ctx)?)
}

/// Return the template source: use the file from `template_dir` if it
/// exists there, otherwise fall back to the embedded built-in.
fn resolve(
    name: &str,
    builtin: &str,
    template_dir: Option<&Path>,
) -> Result<String, ChronicleError> {
    if let Some(dir) = template_dir {
        let path = dir.join(name);
        if path.exists() {
            return Ok(std::fs::read_to_string(path)?);
        }
    }
    Ok(builtin.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    use crate::{DateRange, Summary};

    fn sample_commit(with_diff: bool) -> Commit {
        Commit {
            sha: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            author: "Alice <alice@example.com>".to_string(),
            timestamp: Utc::now(),
            message: "feat: add the widget module".to_string(),
            diff: if with_diff {
                Some("+pub fn widget() -> &'static str { \"hello\" }".to_string())
            } else {
                None
            },
        }
    }

    fn sample_summary() -> Summary {
        Summary {
            text: "Refactored the authentication layer to use JWT tokens.".to_string(),
            commits: vec!["abc".to_string()],
            authors: vec!["Alice".to_string(), "Bob".to_string()],
            date_range: DateRange::new(Utc::now(), Utc::now()),
            model: "llama3".to_string(),
            pass: 1,
            duration_ms: 0,
        }
    }

    // ── render_batch ───────────────────────────────────────────────────────

    #[test]
    fn batch_renders_non_empty() {
        let commits = vec![sample_commit(false)];
        let out = render_batch(&commits, false, None).expect("render");
        assert!(!out.is_empty());
    }

    #[test]
    fn batch_contains_sha_prefix() {
        let commit = sample_commit(false);
        let sha_prefix = commit.sha[..8].to_string();
        let out = render_batch(&[commit], false, None).expect("render");
        assert!(out.contains(&*sha_prefix), "output should contain SHA prefix\n{out}");
    }

    #[test]
    fn batch_contains_author_and_message() {
        let commit = sample_commit(false);
        let out = render_batch(&[commit], false, None).expect("render");
        assert!(out.contains("Alice"), "should contain author name\n{out}");
        assert!(out.contains("feat: add the widget module"), "should contain message\n{out}");
    }

    #[test]
    fn batch_include_diffs_true_shows_diff() {
        let commit = sample_commit(true);
        let out = render_batch(&[commit], true, None).expect("render");
        assert!(out.contains("widget"), "diff content should appear\n{out}");
    }

    #[test]
    fn batch_include_diffs_false_suppresses_diff_when_some() {
        let commit = sample_commit(true); // diff is Some(...)
        let out = render_batch(&[commit], false, None).expect("render");
        assert!(
            !out.contains("pub fn widget"),
            "diff content must be suppressed when include_diffs=false\n{out}"
        );
    }

    #[test]
    fn batch_diff_none_never_appears() {
        let commit = sample_commit(false); // diff is None
        let out = render_batch(&[commit], true, None).expect("render");
        assert!(!out.contains("pub fn widget"), "no diff to show\n{out}");
    }

    // ── render_reduce ──────────────────────────────────────────────────────

    #[test]
    fn reduce_renders_non_empty() {
        let summaries = vec![sample_summary()];
        let out = render_reduce(&summaries, 1, false, None).expect("render");
        assert!(!out.is_empty());
    }

    #[test]
    fn reduce_contains_summary_text_and_authors() {
        let summary = sample_summary();
        let out = render_reduce(&[summary], 1, false, None).expect("render");
        assert!(out.contains("authentication layer"), "should contain summary text\n{out}");
        assert!(out.contains("Alice"), "should contain author name\n{out}");
        assert!(out.contains("Bob"), "should contain author name\n{out}");
    }

    #[test]
    fn reduce_is_final_true_says_narrative() {
        let summaries = vec![sample_summary()];
        let out = render_reduce(&summaries, 1, true, None).expect("render");
        assert!(out.contains("narrative"), "final pass should request a narrative\n{out}");
    }

    #[test]
    fn reduce_is_final_false_says_intermediate() {
        let summaries = vec![sample_summary()];
        let out = render_reduce(&summaries, 1, false, None).expect("render");
        assert!(
            out.contains("intermediate"),
            "non-final pass should request an intermediate summary\n{out}"
        );
    }

    #[test]
    fn reduce_final_and_intermediate_differ() {
        let summaries = vec![sample_summary()];
        let final_out = render_reduce(&summaries, 1, true, None).expect("render final");
        let inter_out = render_reduce(&summaries, 1, false, None).expect("render intermediate");
        assert_ne!(final_out, inter_out);
    }

    // ── custom template override ───────────────────────────────────────────

    #[test]
    fn custom_batch_template_overrides_builtin() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("batch.tera"),
            "CUSTOM_BATCH:{{ commits | length }}",
        )
        .expect("write");

        let out = render_batch(&[sample_commit(false)], false, Some(dir.path()))
            .expect("render");
        assert!(out.starts_with("CUSTOM_BATCH:"), "custom template not used\n{out}");
    }

    #[test]
    fn custom_reduce_template_overrides_builtin() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("reduce.tera"),
            "CUSTOM_REDUCE:{{ summaries | length }}",
        )
        .expect("write");

        let out = render_reduce(&[sample_summary()], 1, false, Some(dir.path()))
            .expect("render");
        assert!(out.starts_with("CUSTOM_REDUCE:"), "custom template not used\n{out}");
    }

    #[test]
    fn missing_custom_file_falls_back_to_builtin() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Only batch.tera is provided; reduce.tera should fall back to built-in
        std::fs::write(
            dir.path().join("batch.tera"),
            "CUSTOM:{{ commits | length }}",
        )
        .expect("write");

        let out = render_reduce(&[sample_summary()], 1, false, Some(dir.path()))
            .expect("render");
        // Built-in reduce.tera contains "intermediate"
        assert!(out.contains("intermediate"), "should have fallen back to built-in\n{out}");
    }
}
