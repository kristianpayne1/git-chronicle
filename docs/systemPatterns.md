# System Patterns — git-chronicle

## Architecture

Data flows in one direction through discrete stages. Stages communicate only through typed data — no shared mutable state. `main.rs` wires the top-level stages; `reducer.rs` internally orchestrates `batcher.rs` as an implementation detail.

```
┌─────────┐    ┌───────────┐    ┌─────────────────────────────────────┐
│   CLI   │───▶│  Ingester │───▶│              Reducer                │
│  (clap) │    │  (git2)   │    │  Batcher ──▶ llm/ (Ollama│Claude)  │
└─────────┘    └───────────┘    └──────────────────┬──────────────────┘
                                                   │
                                   ┌───────────────▼───────────────┐
                                   │         Output Layer          │
                                   │  stdout narrative + JSON trail│
                                   └───────────────────────────────┘
```

## Module Structure

```
src/
├── main.rs        — Entry point. Wires CLI args into the pipeline.
├── cli.rs         — Clap argument definitions and validation logic.
├── ingester.rs    — git2 repo access. Produces Vec<Commit>.
├── batcher.rs     — Chunks commits/summaries into groups of N.
├── templates.rs   — Tera environment setup. Renders batch.tera / reduce.tera.
├── llm/
│   ├── mod.rs     — LlmBackend trait + factory function (constructs backend from config).
│   ├── ollama.rs  — OllamaBackend: native Ollama HTTP API.
│   └── claude.rs  — ClaudeBackend: Anthropic Messages API.
├── reducer.rs     — The hierarchical reduce loop. Orchestrates passes.
├── audit.rs       — AuditEntry struct, serialisation, optional file write.
└── error.rs       — Unified error type via thiserror.

templates/
├── batch.tera     — Default prompt for raw commits → summary.
└── reduce.tera    — Default prompt for summaries → meta-summary.
```

## Core Data Types

```rust
pub struct Commit {
    pub sha: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub diff: Option<String>,      // None when --no-diffs is set
}

pub struct DateRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

pub struct Summary {
    pub text: String,
    pub commits: Vec<String>,      // SHAs, or synthetic IDs on reduce passes
    pub authors: Vec<String>,
    pub date_range: DateRange,
    pub model: String,
    pub pass: u32,
}

pub struct AuditEntry {
    pub pass: u32,
    pub commits: Vec<String>,
    pub authors: Vec<String>,
    pub date_range: DateRange,
    pub model: String,
    pub summary: String,
}
```

## Key Patterns

### Pipeline with typed stages
Each stage takes a concrete input type and returns a concrete output type. No shared mutable state between stages.

### Hierarchical reduce (tree fold)
At each pass:
1. Take `Vec<Summary>` (or `Vec<Commit>` on pass 1)
2. Chunk into groups of `group_size`
3. Send each group to the LLM concurrently
4. Collect results as a new `Vec<Summary>`
5. If `len > 1`, recurse. If `len == 1`, return.

The same model is used for all passes. Infinite loops are impossible — each pass strictly reduces input length, and `group_size >= 2` is enforced at the CLI.

### Concurrency model
- **Within a pass:** all batches sent concurrently via `tokio::task::JoinSet`
- **Between passes:** sequential — pass N+1 cannot begin until pass N is complete

### Templates as a boundary
All prompt construction happens in `templates.rs`. The LLM client never builds strings. Prompt iteration requires no changes to business logic.

### Unified error type
All modules return `Result<T, ChronicleError>`. Variants are defined per module via `thiserror`. `main.rs` catches at the top and prints a clean message. No `unwrap()` or `expect()` outside of tests.

### LLM backend abstraction (trait)
`llm/mod.rs` defines a single async trait:

```rust
#[async_trait]
pub trait LlmBackend: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String, ChronicleError>;
}
```

`main.rs` calls the factory function at startup to get a `Box<dyn LlmBackend>`, which is then threaded through the pipeline:

```rust
// llm/mod.rs
pub fn build(config: &Config) -> Box<dyn LlmBackend> {
    match config.backend {
        Backend::Ollama => Box::new(OllamaBackend::new(config)),
        Backend::Claude => Box::new(ClaudeBackend::new(config)),
    }
}
```

Each backend speaks its own native protocol with no translation layer:
- `OllamaBackend` — POSTs to Ollama's `/api/generate`, handles Ollama-specific error shapes and retry behaviour
- `ClaudeBackend` — POSTs to `/v1/messages`, handles 429/529 retries, injects `anthropic-version` header and `ANTHROPIC_API_KEY`

Only `reducer.rs` holds a `&dyn LlmBackend` and calls `.complete(prompt)`. `batcher.rs` only chunks data and has no LLM dependency. `reducer.rs` is entirely unaware of which backend is active. In tests, a `MockBackend` implements the same trait — no HTTP mocking required.

### Incremental audit trail
Each `AuditEntry` is written to disk as it is produced — not buffered until the end. An interrupted run preserves partial results.

## Template Variables

**`batch.tera`** receives:
```
commits[]
  .sha, .author, .timestamp, .message, .diff
```

**`reduce.tera`** receives:
```
summaries[]
  .text, .authors, .date_range.from, .date_range.to
pass          — current pass number (integer)
is_final      — true on the last pass (boolean)
              — use to adjust tone: final pass should produce a complete narrative,
                intermediate passes should produce a dense intermediate summary
```

Custom templates supplied via `--template <dir>` override the built-in defaults. Missing files in the override directory fall back to built-ins.
