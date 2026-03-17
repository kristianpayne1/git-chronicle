pub mod audit;
pub mod batcher;
pub mod cli;
pub mod error;
pub mod ingester;
pub mod llm;
pub mod reducer;
pub mod templates;
pub mod types;

pub use error::ChronicleError;
pub use types::{AuditEntry, Commit, DateRange, Summary};

use std::collections::HashMap;

use chrono::NaiveDate;
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    audit::AuditWriter,
    cli::Cli,
    ingester::Filters,
    reducer::{ProgressEvent, ReduceConfig},
};

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("git-chronicle: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), ChronicleError> {
    let cli = Cli::parse();
    cli.validate()?;

    let path = match &cli.path {
        Some(p) => p.clone(),
        None => std::env::current_dir().map_err(ChronicleError::IoError)?,
    };

    let since = cli
        .since
        .as_deref()
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| {
                ChronicleError::InvalidConfig(format!(
                    "--since must be YYYY-MM-DD, got '{s}'"
                ))
            })
        })
        .transpose()?;

    let filters = Filters {
        authors: cli.author.clone(),
        since,
        branch: cli.branch.clone(),
        from_sha: cli.from.clone(),
        to_sha: cli.to.clone(),
    };

    let include_diffs = !cli.no_diffs;

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    spinner.set_message("Reading commits…");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let commits = ingester::ingest(&path, &filters, include_diffs)?;
    spinner.finish_and_clear();

    if commits.is_empty() {
        eprintln!("git-chronicle: no commits found matching the given filters.");
        return Ok(());
    }

    let model = cli.model.clone();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let progress_handle = tokio::spawn(run_progress(rx));

    let config = ReduceConfig {
        group_size: cli.group_size,
        include_diffs,
        template_dir: cli.template.clone(),
        model,
        progress: Some(tx),
    };

    let mut audit = AuditWriter::new(cli.output.as_deref())?;
    let backend = llm::build(&cli);

    let overall_start = std::time::Instant::now();
    let narrative = reducer::reduce(commits, backend, &mut audit, &config).await?;
    let overall_ms = overall_start.elapsed().as_millis();

    // Dropping config closes the progress channel; then we wait for bars to clear.
    drop(config);
    progress_handle
        .await
        .map_err(|e| ChronicleError::LlmFailure(format!("progress task panicked: {e}")))?;

    eprintln!("Total: {:.1}s", overall_ms as f64 / 1000.0);
    println!("{narrative}");
    Ok(())
}


async fn run_progress(mut rx: UnboundedReceiver<ProgressEvent>) {
    let mp = MultiProgress::new();
    let style = ProgressStyle::with_template("{spinner:.cyan} {msg} [{bar:40.cyan/blue}] {pos}/{len}")
        .unwrap_or_else(|_| ProgressStyle::default_bar());
    let mut bars: HashMap<u32, ProgressBar> = HashMap::new();
    let mut pass_times: Vec<(u32, u64)> = Vec::new();

    while let Some(event) = rx.recv().await {
        match event {
            ProgressEvent::PassStarted { pass, total } => {
                let pb = mp.add(ProgressBar::new(total as u64));
                pb.set_style(style.clone());
                pb.set_message(format!("Pass {pass} — {total} batches"));
                pb.enable_steady_tick(std::time::Duration::from_millis(80));
                bars.insert(pass, pb);
            }
            ProgressEvent::BatchCompleted { pass } => {
                if let Some(pb) = bars.get(&pass) {
                    pb.inc(1);
                }
            }
            ProgressEvent::PassFinished { pass, duration_ms } => {
                if let Some(pb) = bars.get(&pass) {
                    pb.finish_and_clear();
                }
                pass_times.push((pass, duration_ms));
            }
        }
    }

    for pb in bars.values() {
        pb.finish_and_clear();
    }
    mp.clear().ok();

    for (pass, ms) in &pass_times {
        eprintln!("Pass {pass}: {:.1}s", *ms as f64 / 1000.0);
    }
}
