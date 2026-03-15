# Implementation Plan — git-chronicle

## Overview

Step-by-step plan from an empty directory to a working MVP. Steps are ordered by dependency — each step builds directly on the last. Every step is considered **incomplete** until all unit tests are written and passing. Every step ends with an update to `docs/progress.md`.

## Rules for Every Step

- **Tests first:** write unit tests alongside implementation. The step is not done until `cargo test` passes.
- **No `unwrap()` or `expect()`** outside of `#[cfg(test)]` blocks.
- **No `unsafe`** without an explicit comment explaining why.
- **Update `docs/progress.md`** at the end of every step.
- **Update the relevant doc file** (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`) if implementation reveals any discrepancy or requires a design decision not captured there.

## `docs/progress.md` Structure

Each step appends to or updates this file. It must always contain:

```
## What Works
- <bulleted list of completed, tested functionality>

## What's Left
- <bulleted list of remaining steps>

## Current Status
<one-line summary of where the project is right now>
```

---

## Step 1 — Project Scaffold

**Goal:** Initialise the Rust project, configure all dependencies, and create empty module stubs. The project must compile cleanly before any real logic is written.

### RISEN Prompt

**Role:** You are an expert Rust developer bootstrapping a new CLI project from scratch.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing anything — the scaffold must reflect all of them
- Run `cargo new git-chronicle --bin` in the project directory
- Configure `Cargo.toml` with all runtime and dev dependencies from `techContext.md` at the versions specified
- Configure `git2` with `features = ["vendored"]` for static linking
- Configure `tokio` with `features = ["full"]`
- Create empty stub files for every module in `src/`:
  - `cli.rs`, `ingester.rs`, `batcher.rs`, `templates.rs`, `reducer.rs`, `audit.rs`, `error.rs`
  - `llm/mod.rs`, `llm/ollama.rs`, `llm/claude.rs`
- Each stub contains a `// TODO` comment and declares its module publicly in `main.rs`
- Create `templates/batch.tera` and `templates/reduce.tera` as empty files
- `main.rs` must compile and exit with code 0

**Situation:** Empty directory. All specification is in `docs/`.

**Expectations:**
- `cargo build` succeeds with zero errors and zero warnings
- `cargo test` runs and reports no failures
- Directory structure matches `systemPatterns.md` exactly

**Narrowing:**
- Use the exact crate versions from `techContext.md`
- Do not add any crates not listed in `techContext.md`
- No `unwrap()` even in stubs — stubs should be empty or return `todo!()`

### Definition of Done
- [ ] `cargo build` passes clean
- [ ] `cargo test` passes
- [ ] All module stub files exist in the correct locations
- [ ] `docs/progress.md` created and initialised

---

## Step 2 — Error Types (`error.rs`)

**Goal:** Define `ChronicleError` — the single unified error type used by every module.

### RISEN Prompt

**Role:** You are an expert Rust developer implementing ergonomic, user-facing error handling for a CLI tool.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Define `ChronicleError` in `error.rs` using `#[derive(thiserror::Error, Debug)]`
- Include variants for every failure mode across the application:
  - `GitError(#[from] git2::Error)` — repository access failures
  - `LlmFailure(String)` — LLM call failed or exhausted retries
  - `TemplateError(#[from] tera::Error)` — template rendering failure
  - `IoError(#[from] std::io::Error)` — file read/write failure
  - `InvalidConfig(String)` — bad CLI argument or argument combination
  - `SerializationError(#[from] serde_json::Error)` — JSON serialisation failure
- Every variant must have a `#[error("...")]` message written for a non-technical user
- Re-export `ChronicleError` from the crate root so all modules use `use crate::ChronicleError`

**Situation:** Project scaffold compiles. No logic exists yet.

**Expectations:**
- `error.rs` compiles with no warnings
- Unit tests construct each variant and assert the `Display` output matches expectations
- `ChronicleError` is usable as a `Box<dyn std::error::Error>`

**Narrowing:**
- No manual `impl std::error::Error` — use `thiserror` exclusively
- Error messages are end-user facing, not internal debug strings
- No `unwrap()` or `expect()`

### Definition of Done
- [ ] All variants defined with user-facing messages
- [ ] Unit tests for `Display` output of each variant pass
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 3 — Core Data Types

**Goal:** Define `Commit`, `DateRange`, `Summary`, and `AuditEntry` — the typed data that flows between every stage.

### RISEN Prompt

**Role:** You are an expert Rust developer defining the core data model for a multi-stage data pipeline.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Define all four structs in a `types.rs` module, re-exporting from `lib.rs` or directly from `main.rs` as appropriate
- Derive `#[derive(Debug, Clone, Serialize, Deserialize)]` on all four structs
- Match the field definitions exactly as specified in `systemPatterns.md`
- Add `DateRange::new(from: DateTime<Utc>, to: DateTime<Utc>) -> Self`
- Add `impl From<&Summary> for AuditEntry` for convenient conversion in the reducer
- All fields must be `pub`
- `Commit.diff` must be `Option<String>` and must serialise as `null` when absent

**Situation:** Error types exist. Project compiles cleanly.

**Expectations:**
- All structs compile with serde derives
- Unit tests verify round-trip JSON serialisation for each type
- `AuditEntry::from(&summary)` produces correct field mapping
- `null` diff serialises and deserialises correctly

**Narrowing:**
- Use `chrono::DateTime<Utc>` for all timestamps — no `NaiveDateTime`
- No `unwrap()` or `expect()`

### Definition of Done
- [ ] All four structs defined with correct fields and derives
- [ ] Round-trip JSON serialisation tests pass for all types
- [ ] `From<&Summary> for AuditEntry` implemented and tested
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 4 — CLI Parsing (`cli.rs`)

**Goal:** Implement the full CLI surface using clap's derive API, including all flags and validation.

### RISEN Prompt

**Role:** You are an expert Rust developer implementing a CLI interface using clap 4.x derive macros.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Define a `Cli` struct with `#[derive(Parser)]` covering every flag in the CLI flags reference table in `techContext.md`
- Define a `Backend` enum (`Ollama`, `Claude`) with `#[derive(Clone, Debug, PartialEq)]` used by `--backend`
- `<path>` is an optional positional argument defaulting to CWD
- Implement `Cli::validate(&self) -> Result<(), ChronicleError>` that checks:
  - `--group-size` >= 2
  - `--backend claude` requires `ANTHROPIC_API_KEY` env var to be set
  - `--from` and `--to`, if provided, are valid 40-character lowercase hex strings
  - If `--template` is provided, the path exists and is a directory
  - `<path>` resolves to a directory that contains a `.git` folder
- Expose a `Config` struct (or use `Cli` directly) that downstream modules accept

**Situation:** Error types and data types exist. No ingester or LLM logic yet.

**Expectations:**
- `--help` output is clean and documents all flags with defaults
- Unit tests cover every validation rule:
  - `--backend claude` without `ANTHROPIC_API_KEY` → `InvalidConfig`
  - `--group-size 1` → `InvalidConfig`
  - `--from` with non-hex string → `InvalidConfig`
  - Non-existent `--template` path → `InvalidConfig`

**Narrowing:**
- Use clap `value_parser`, `default_value_t`, and `long_help` where appropriate
- All validation errors go through `ChronicleError::InvalidConfig` with a clear message
- No `unwrap()` or `expect()`

### Definition of Done
- [ ] All flags implemented and parse correctly
- [ ] `validate()` covers every invalid combination
- [ ] Unit tests for all validation branches pass
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 5 — Ingester (`ingester.rs`)

**Goal:** Read commits from a git repository via `git2` and apply all filters.

### RISEN Prompt

**Role:** You are an expert Rust developer implementing git repository traversal using libgit2 bindings.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Implement `pub fn ingest(path: &Path, filters: &Filters, include_diffs: bool) -> Result<Vec<Commit>, ChronicleError>`
- `Filters` holds: `authors: Vec<String>`, `since: Option<NaiveDate>`, `branch: Option<String>`, `from_sha: Option<String>`, `to_sha: Option<String>`
- Open the repo with `git2::Repository::open(path)`
- Configure a `Revwalk` starting from the specified branch or HEAD
- If `from_sha` is set, start the walk from that commit (inclusive)
- If `to_sha` is set, stop at that commit (inclusive)
- Filter by author: match against commit author name or email (case-insensitive)
- Filter by date: exclude commits before `--since`
- Load diffs: if `include_diffs` is true, generate a diff for each commit and capture as a `String`; drop the git2 diff object immediately after — never accumulate all diffs simultaneously
- Return commits in chronological order (oldest first)
- Return a clear `ChronicleError::GitError` if the path is not a valid git repo

**Situation:** CLI, error types, and data types all exist. No LLM or template logic yet.

**Expectations:**
- Integration tests use `tempfile` and `git2` to create an in-memory git repo with known commits
- Tests cover: no filters (all commits returned), author filter, date filter, SHA range filter, `include_diffs` true and false
- Tests verify chronological ordering
- Tests verify diff is `None` when `include_diffs` is false

**Narrowing:**
- `git2` statically linked — `features = ["vendored"]` in `Cargo.toml`
- Never accumulate all diffs in memory at once
- No `unwrap()` or `expect()`
- All `git2::Error` values convert via `#[from]` on `ChronicleError::GitError`

### Definition of Done
- [ ] `ingest()` implemented with all filters
- [ ] Integration tests with real (tempfile) git repos pass
- [ ] Diff lazy-loading confirmed by test
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 6 — Batcher (`batcher.rs`)

**Goal:** Implement chunking logic for both commits (pass 1) and summaries (subsequent passes).

### RISEN Prompt

**Role:** You are an expert Rust developer implementing a pure, well-tested chunking utility.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Implement `pub fn batch_commits(commits: Vec<Commit>, group_size: usize) -> Vec<Vec<Commit>>`
- Implement `pub fn batch_summaries(summaries: Vec<Summary>, group_size: usize) -> Vec<Vec<Summary>>`
- Both functions split their input into consecutive groups of at most `group_size`
- The final group may contain fewer than `group_size` items
- Both functions return an empty `Vec` if given empty input
- `group_size` is guaranteed >= 2 by CLI validation; use `debug_assert!(group_size >= 2)` at the top of each function

**Situation:** Data types, error types, CLI, and ingester all exist and are tested.

**Expectations:**
- Unit tests cover:
  - Empty input → empty output
  - Input length < `group_size` → one group containing all items
  - Input length exactly equal to `group_size` → one full group
  - Input length > `group_size` → multiple groups, last may be smaller
  - Input length exactly divisible by `group_size` → all groups equal size

**Narrowing:**
- Pure functions — no I/O, no async, no errors, no side effects
- No `unwrap()` or `expect()`
- Do not clone items unnecessarily — consume the input `Vec`

### Definition of Done
- [ ] Both batch functions implemented
- [ ] All edge case unit tests pass
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 7 — Templates (`templates.rs`)

**Goal:** Set up Tera, embed the built-in templates in the binary, implement rendering functions, and support custom template override.

### RISEN Prompt

**Role:** You are an expert Rust developer implementing a prompt rendering system using the Tera template engine.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Embed `templates/batch.tera` and `templates/reduce.tera` at compile time using `include_str!()`
- Write `batch.tera` to render: for each commit, output sha (short), author, timestamp (formatted), message, and — only if diff is `Some` — the diff content
- Write `reduce.tera` to render: for each summary, output date range, authors, and text; include `pass` number and use `is_final` to adjust instructions (final pass: "write a complete narrative"; intermediate: "write a dense intermediate summary")
- Implement `pub fn render_batch(commits: &[Commit], include_diffs: bool) -> Result<String, ChronicleError>`
- Implement `pub fn render_reduce(summaries: &[Summary], pass: u32, is_final: bool) -> Result<String, ChronicleError>`
- If a `template_dir: Option<PathBuf>` is provided, load from that directory first; fall back to built-ins for any missing file

**Situation:** Data types and error types exist. No LLM or reducer logic yet.

**Expectations:**
- Unit tests render both templates with sample data and assert output is non-empty and contains expected fields
- Test that `include_diffs: false` suppresses diff content even when `Commit.diff` is `Some`
- Test that `is_final: true` produces different output than `is_final: false`
- Test custom template override for both files

**Narrowing:**
- Built-in templates ship inside the binary — no runtime file reads for defaults
- Template errors map to `ChronicleError::TemplateError`
- No `unwrap()` or `expect()`

### Definition of Done
- [ ] Both built-in templates produce valid rendered output
- [ ] `include_diffs` suppression tested
- [ ] `is_final` tone difference tested
- [ ] Custom template override tested
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 8 — LLM Trait and MockBackend (`llm/mod.rs`)

**Goal:** Define the `LlmBackend` async trait and a `MockBackend` for use in all downstream tests.

### RISEN Prompt

**Role:** You are an expert Rust developer designing a testable async trait abstraction for an LLM client.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Define in `llm/mod.rs`:
  ```rust
  #[async_trait]
  pub trait LlmBackend: Send + Sync {
      async fn complete(&self, prompt: &str) -> Result<String, ChronicleError>;
  }
  ```
- Define `pub fn build(config: &Config) -> Box<dyn LlmBackend>` that matches on `config.backend`:
  - `Backend::Ollama` → `Box::new(OllamaBackend::new(config))`
  - `Backend::Claude` → `Box::new(ClaudeBackend::new(config))`
- Implement `MockBackend` available in `#[cfg(test)]`:
  - Constructor takes `Vec<Result<String, ChronicleError>>` as canned responses
  - Returns them in order on each `complete()` call
  - Panics with a clear message if called more times than responses provided
  - Exposes `call_count(&self) -> usize` for assertions

**Situation:** All data types, error types, CLI, ingester, batcher, and templates exist and are tested.

**Expectations:**
- Unit tests verify `MockBackend` returns responses in order
- Unit tests verify `MockBackend` panics correctly on exhaustion
- `build()` routes to the correct backend type

**Narrowing:**
- `MockBackend` must only exist in test code — use `#[cfg(test)]` or a test module
- Factory must use `config.backend` enum — no env var inspection
- No `unwrap()` or `expect()` in production code

### Definition of Done
- [ ] `LlmBackend` trait defined and compiles
- [ ] `MockBackend` implemented and tested
- [ ] `build()` factory function routes correctly
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 9 — OllamaBackend (`llm/ollama.rs`)

**Goal:** Implement the Ollama HTTP backend with retry logic.

### RISEN Prompt

**Role:** You are an expert Rust developer implementing a resilient async HTTP client for the Ollama API.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Implement `OllamaBackend { client: reqwest::Client, endpoint: String, model: String }`
- `complete()` sends POST to `{endpoint}/api/generate`:
  ```json
  { "model": "<model>", "prompt": "<prompt>", "stream": false }
  ```
- Parse response field `"response"` as the returned text
- **Retry logic:** on connection error or timeout, wait 1 second and retry once; if the retry also fails, return `ChronicleError::LlmFailure`
- **Non-200 response:** surface immediately as `ChronicleError::LlmFailure("<status>: <body>")` — do not retry
- Create `reqwest::Client` once in `OllamaBackend::new()` and reuse across calls

**Situation:** LLM trait and `MockBackend` exist. All other modules exist.

**Expectations:**
- Integration tests use `mockito` to serve fake Ollama responses:
  - Happy path: correct JSON returned, `response` field extracted
  - Retry path: first request fails with connection error, second succeeds
  - Non-200: 500 response surfaces immediately without retry
- Tests confirm no third request is made after a successful retry

**Narrowing:**
- No `unwrap()` or `expect()`
- `reqwest` errors map to `ChronicleError::LlmFailure`
- Endpoint is read from the struct — never hardcoded

### Definition of Done
- [ ] `OllamaBackend` implements `LlmBackend`
- [ ] Happy path tested with `mockito`
- [ ] Retry behaviour tested
- [ ] Non-200 error handling tested
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 10 — ClaudeBackend (`llm/claude.rs`)

**Goal:** Implement the Anthropic Messages API backend with correct headers and retry logic.

### RISEN Prompt

**Role:** You are an expert Rust developer implementing a resilient async HTTP client for the Anthropic Claude API.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Implement `ClaudeBackend { client: reqwest::Client, endpoint: String, model: String, api_key: String }`
- `complete()` sends POST to `{endpoint}/v1/messages` with:
  - Header `x-api-key: <api_key>`
  - Header `anthropic-version: 2023-06-01`
  - Header `content-type: application/json`
  - Body:
    ```json
    {
      "model": "<model>",
      "max_tokens": 4096,
      "messages": [{ "role": "user", "content": "<prompt>" }]
    }
    ```
- Parse response: extract `content[0].text`
- **Retry logic:** on 429 or 529 response, wait 5 seconds and retry once; if retry fails, return `ChronicleError::LlmFailure`
- **Other errors:** surface immediately as `ChronicleError::LlmFailure` — do not include the API key in the message

**Situation:** `OllamaBackend` exists. Full LLM trait established. All other modules tested.

**Expectations:**
- Integration tests use `mockito`:
  - Happy path: correct headers sent, `content[0].text` extracted
  - 429 retry: first returns 429, second succeeds — assert exactly two requests made
  - 500 error: surfaces immediately, no retry
  - API key must not appear in any error message (assert in tests)

**Narrowing:**
- `max_tokens` is hardcoded at 4096 in v0.1
- API key must never appear in error messages or log output
- No `unwrap()` or `expect()`

### Definition of Done
- [ ] `ClaudeBackend` implements `LlmBackend`
- [ ] All required headers verified in tests
- [ ] 429/529 retry tested
- [ ] API key redaction from error messages confirmed
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 11 — Reducer (`reducer.rs`)

**Goal:** Implement the hierarchical reduce loop that orchestrates batching and LLM calls across all passes.

### RISEN Prompt

**Role:** You are an expert Rust developer implementing a recursive async tree-fold pipeline.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Implement:
  ```rust
  pub async fn reduce(
      commits: Vec<Commit>,
      backend: &dyn LlmBackend,
      audit: &mut AuditWriter,
      config: &ReduceConfig,
  ) -> Result<String, ChronicleError>
  ```
- `ReduceConfig` holds: `group_size: usize`, `include_diffs: bool`, `template_dir: Option<PathBuf>`
- **Pass 1:** batch commits via `batcher::batch_commits` → render each group via `templates::render_batch` → call `backend.complete()` concurrently for each group using `tokio::task::JoinSet` → collect results into `Vec<Summary>`
- **Pass N (N > 1):** batch summaries via `batcher::batch_summaries` → determine `is_final` (true when batches produce exactly one group) → render each group via `templates::render_reduce` → call `backend.complete()` concurrently → collect into new `Vec<Summary>`
- After each pass, call `audit.record()` for each completed `Summary`
- Recurse until `Vec<Summary>` has length 1; return its text
- Assert that each pass strictly reduces the number of items (panic in debug builds if not)

**Situation:** All modules exist and are independently tested. `MockBackend` is available for testing.

**Expectations:**
- Unit tests using `MockBackend` verify:
  - Single group of commits → one LLM call → narrative returned directly
  - Multiple groups requiring two passes → correct number of LLM calls, correct pass numbering
  - `is_final` is `true` only on the last pass
  - `audit.record()` called once per `Summary` produced
- No real HTTP calls in any test

**Narrowing:**
- No `unwrap()` or `expect()` outside tests
- Infinite loop is impossible — assert input length strictly decreases each pass
- `group_size >= 2` guaranteed by CLI

### Definition of Done
- [ ] Single-pass and multi-pass reduction tested with `MockBackend`
- [ ] `is_final` set correctly in all cases
- [ ] `audit.record()` called correctly
- [ ] Concurrent execution within a pass confirmed
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 12 — Audit Trail (`audit.rs`)

**Goal:** Implement incremental, newline-delimited JSON audit trail writing.

### RISEN Prompt

**Role:** You are an expert Rust developer implementing an append-only structured log file.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Implement `AuditWriter` struct
- `AuditWriter::new(path: Option<PathBuf>) -> Result<Self, ChronicleError>`:
  - If `path` is `None`, create a no-op writer
  - If `path` is `Some`, open (or create) the file for append
- `AuditWriter::record(&mut self, entry: &AuditEntry) -> Result<(), ChronicleError>`:
  - If no-op: return `Ok(())`
  - Otherwise: serialise `entry` to a single JSON line and write it followed by `\n`
  - Flush after each write — do not buffer across entries
- `AuditWriter::finish(&mut self) -> Result<(), ChronicleError>`:
  - Flush and close the file if open

**Situation:** All other modules implemented and tested.

**Expectations:**
- Unit tests (using `tempfile`):
  - Write multiple entries, read file back, assert each line is valid JSON and round-trips correctly
  - No-op writer (`path: None`) produces no file and no error
  - Partial write (simulate early stop) leaves all written entries readable

**Narrowing:**
- Output format is newline-delimited JSON (one JSON object per line)
- Write must be incremental — never buffer all entries in memory
- File I/O errors map to `ChronicleError::IoError`
- No `unwrap()` or `expect()`

### Definition of Done
- [ ] `AuditWriter` implemented
- [ ] Round-trip tests pass
- [ ] No-op behaviour tested
- [ ] Partial write durability tested
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated

---

## Step 13 — Main Pipeline (`main.rs`)

**Goal:** Wire all modules together, add progress bars, and produce the final narrative.

### RISEN Prompt

**Role:** You are an expert Rust developer assembling a complete async CLI pipeline from independently tested modules.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before writing any code
- Annotate `main` with `#[tokio::main]`
- Parse args with `Cli::parse()`, call `validate()`, exit on error with a clear `eprintln!` and code 1
- Build `Box<dyn LlmBackend>` via `llm::build(&config)`
- Call `ingester::ingest()` — if 0 commits returned, print a clear message and exit 0
- Initialise `AuditWriter` with `--output` path
- Add `indicatif` progress bars:
  - One multi-progress bar container
  - Add a new bar per reduce pass labelled "Pass N — X batches"
  - Each batch completion ticks the bar
  - All bars cleared before the final narrative is printed to stdout
- Call `reducer::reduce()` passing backend, audit writer, and config
- Print the final narrative to stdout
- Call `audit.finish()`
- All top-level errors caught and printed to stderr — exit code 1

**Situation:** All modules implemented and unit tested. Ready to wire together.

**Expectations:**
- `assert_cmd` integration tests run the compiled binary:
  - Against a `tempfile` git repo with a known commit history
  - With a `--backend` flag or environment shim that injects `MockBackend` responses, or using a local Ollama instance if available
  - Assert stdout contains non-empty prose
  - Assert exit code 0 on success, 1 on error
  - Assert `--output` produces a file with one JSON line per summary
- Progress bars render to stderr without corrupting stdout

**Narrowing:**
- Progress bars go to stderr — stdout must contain only the narrative
- No `unwrap()` or `expect()`
- `main()` catches all errors at the top level — no error should propagate to a Rust panic

### Definition of Done
- [ ] Full pipeline runs end-to-end against a real git repo
- [ ] `assert_cmd` integration tests pass
- [ ] Progress bars do not corrupt stdout
- [ ] Correct exit codes on all error paths
- [ ] `--output` audit file written and valid
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated with MVP complete status

---

## Step 14 — End-to-End Validation

**Goal:** Run the complete binary against real repositories under real conditions and fix any issues found.

### RISEN Prompt

**Role:** You are a developer validating an MVP CLI tool against real-world conditions before calling it done.

**Instructions:**
- Read all files in `docs/` (`projectbrief.md`, `productContext.md`, `techContext.md`, `systemPatterns.md`, `progress.md`) before starting validation
- Run `git-chronicle` against the `git-chronicle` repo itself (dogfooding)
- Run against at least one other repo with substantial history
- Exercise every CLI flag at least once:
  - `--author`, `--since`, `--from`, `--to`, `--branch`
  - `--no-diffs`
  - `--group-size` with a non-default value
  - `--output` to produce an audit trail
  - `--backend claude` if `ANTHROPIC_API_KEY` is available
- Inspect the output narrative for coherence and prose quality
- Inspect the audit JSON for correct metadata
- Document any bugs, panics, or unexpected output
- For each bug found: fix it and add a regression test before marking the step done

**Situation:** Binary compiles and all unit/integration tests pass. Requires local Ollama running with `qwen3.5:9b`.

**Expectations:**
- No panics or crashes on any tested input
- Output reads as coherent prose, not a bullet list
- Audit trail is valid newline-delimited JSON
- All regression tests for bugs found are written and passing

**Narrowing:**
- This step requires Ollama running locally
- Do not mark complete with known bugs — fix them first
- Document findings in `docs/progress.md`

### Definition of Done
- [ ] Binary runs successfully against `git-chronicle` repo
- [ ] All CLI flags exercised in a real run
- [ ] No panics or crashes observed
- [ ] Output reads as coherent prose
- [ ] All bugs found have regression tests
- [ ] `cargo test` passes
- [ ] `docs/progress.md` updated with final MVP status
