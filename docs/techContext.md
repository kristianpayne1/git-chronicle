# Tech Context — git-chronicle

## Rust Toolchain

| Property          | Value                          |
|-------------------|--------------------------------|
| Edition           | 2021                           |
| MSRV              | 1.75                           |
| Primary target    | `aarch64-apple-darwin`         |
| Secondary target  | `x86_64-unknown-linux-gnu`     |
| Release profile   | `opt-level = 3`, `lto = true`, `strip = true` |

## Dependencies

### Runtime

| Crate          | Purpose                                              |
|----------------|------------------------------------------------------|
| `clap` 4.x     | CLI argument parsing — derive macro API              |
| `git2` 0.18    | libgit2 bindings — commit traversal, diffs, authors  |
| `reqwest` 0.12 | Async HTTP client for LLM backend HTTP calls         |
| `tokio` 1.x    | Async runtime (`features = ["full"]`)                |
| `tera` 1.x     | Jinja2-style prompt template engine                  |
| `serde` 1.x    | Serialisation derive macros                          |
| `serde_json` 1.x | JSON audit trail serialisation                     |
| `indicatif` 0.17 | Progress bars per reduce pass                      |
| `colored` 2.x  | Terminal colour output                               |
| `chrono` 0.4   | Timestamp parsing and formatting                     |
| `thiserror` 1.x | Ergonomic error type derivation                    |
| `async-trait` 0.1 | `#[async_trait]` macro for `LlmBackend` trait     |

### Dev / test only

| Crate        | Purpose                                      |
|--------------|----------------------------------------------|
| `mockito`    | HTTP mock server for OllamaBackend/ClaudeBackend integration tests |
| `tempfile`   | Temporary git repos in integration tests     |
| `assert_cmd` | CLI integration test harness                 |

## LLM Integration

The LLM layer is built around a `LlmBackend` trait. Two implementations ship in v0.1:

### OllamaBackend (default)
- **Endpoint:** `http://localhost:11434` (override via `CHRONICLE_ENDPOINT`)
- **Protocol:** Ollama native API (`/api/generate`), HTTP POST
- **Auth:** none
- **Default model:** `qwen3.5:9b`

```json
{ "model": "qwen3.5:9b", "prompt": "<rendered prompt>", "stream": false }
```
```json
{ "response": "<summary text>" }
```

**Error handling:** retry once with backoff on connection failure, then surface as `ChronicleError::LlmFailure`.

### ClaudeBackend (opt-in)
- **Endpoint:** `https://api.anthropic.com` (default, override via `CHRONICLE_ENDPOINT`)
- **Protocol:** Anthropic Messages API (`/v1/messages`), HTTP POST
- **Auth:** `ANTHROPIC_API_KEY` environment variable (required when `--backend claude` is set)
- **Required header:** `anthropic-version: 2023-06-01`
- **Default model:** `claude-sonnet-4-6`

```json
{
  "model": "claude-sonnet-4-6",
  "max_tokens": 4096,
  "messages": [{ "role": "user", "content": "<rendered prompt>" }]
}
```
```json
{ "content": [{ "type": "text", "text": "<summary text>" }] }
```

**Error handling:** retry once with backoff on 429/529, surface all other errors immediately as `ChronicleError::LlmFailure`.

## Environment Variables

| Variable             | Purpose                                                   | Default                  |
|----------------------|-----------------------------------------------------------|--------------------------|
| `CHRONICLE_ENDPOINT` | LLM endpoint URL                                          | backend-specific default |
| `CHRONICLE_MODEL`    | Override default model                                    | backend-specific default |
| `CHRONICLE_BACKEND`  | Backend selection (`ollama` or `claude`)                  | `ollama`                 |
| `ANTHROPIC_API_KEY`  | API key (required when `--backend claude`)                | unset                    |
| `NO_COLOR`           | Disable coloured output                                   | unset                    |

### CLI flags reference

| Flag / Arg       | Purpose                                                        | Default         |
|------------------|----------------------------------------------------------------|-----------------|
| `<path>`         | Path to the git repository to analyse (positional)             | CWD             |
| `--backend`      | LLM backend (`ollama` or `claude`)                             | `ollama`        |
| `--model`        | Model name passed to the backend                               | backend default |
| `--group-size`   | Number of commits/summaries per batch (min 2)                  | `20`            |
| `--no-diffs`     | Omit diffs from prompts (reduces token usage)                  | off             |
| `--template`     | Directory containing custom `batch.tera` / `reduce.tera`       | built-ins       |
| `--output`       | Path to write the JSON audit trail (omit to skip writing)      | unset           |
| `--author`       | Filter commits by author (repeatable)                          | all authors     |
| `--since`        | Filter commits after this date (`YYYY-MM-DD`)                  | repo start      |
| `--branch`       | Branch to read history from                                    | current branch  |
| `--from`         | Start commit SHA (inclusive); use with or without `--to`       | repo start      |
| `--to`           | End commit SHA (inclusive); use with or without `--from`       | HEAD            |

CLI flags take precedence over environment variables. `--backend` (or `CHRONICLE_BACKEND`) is the single source of truth for backend selection — `ANTHROPIC_API_KEY` alone does not activate `ClaudeBackend`.

## Local Development Setup

```bash
# Prerequisites (macOS) — local runner
brew install ollama
ollama pull qwen3.5:9b       # default batch model
ollama pull llama3.2:latest  # example alternative — swap in via --model

# Build
git clone <repo> git-chronicle && cd git-chronicle
cargo build

# Run against a local repo (default: Ollama + qwen3.5:9b)
cargo run -- --model qwen3.5:9b /path/to/repo

# Optional: use Claude API instead of a local model
ANTHROPIC_API_KEY=sk-... \
cargo run -- --backend claude --model claude-sonnet-4-6 /path/to/repo

# Tests (no LLM required — HTTP calls are mocked)
cargo test
```

## Constraints

- **No `unsafe`** without explicit justification and comment
- **No `unwrap()` or `expect()`** outside of `#[cfg(test)]`
- **Memory:** diffs are loaded lazily and dropped after each batch — never accumulate all diffs at once
- **`git2` statically linked** — no system libgit2 dependency, avoids version mismatches on Linux
- **Token limits** are out of scope — if a prompt exceeds the model's context window, the user tunes `--group-size` or `--no-diffs`
- **No config file** in v0.1 — flags and environment variables only
- **Windows** is not a target in v0.1
