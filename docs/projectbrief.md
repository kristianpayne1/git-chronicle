# Project Brief — git-chronicle

## Overview

`git-chronicle` is a local-first Rust CLI that reads a git repository's commit history and produces a human-readable narrative summary using an LLM. By default it runs entirely offline via a local Ollama instance. Optionally, users can point it at the Claude API for higher-quality output. No telemetry.

## Core Goal

Most git tools show you *what* changed. `git-chronicle` tells you *the story* — who contributed, what their impact was, and how the project evolved over time.

## Requirements

### Functional
- Read commit history from any local git repo (messages, diffs, authors, timestamps)
- Filter commits by author, date range, branch, or commit range (`--from` / `--to`)
- Batch commits and summarise them using a configurable LLM (local Ollama by default, Claude API optionally)
- Apply a hierarchical reduce algorithm until a single narrative remains
- Emit the narrative to stdout
- Optionally write a full JSON audit trail to disk

### Non-functional
- Runs offline by default — no network calls except to the configured LLM endpoint (local Ollama or optional Claude API)
- Single binary distribution
- Compiles on macOS (Apple Silicon) and Linux
- No panics in the happy path — all errors surface cleanly

## Pipeline Phases

1. **Ingest** — Read commits from the target repo via `git2` (defaults to CWD), apply filters
2. **Batch** — Chunk commits into groups of N, render each into a prompt via `batch.tera`
3. **Summarise** — Send each prompt to the configured LLM backend, collect first-pass summaries
4. **Reduce** — Re-batch summaries, re-summarise via `reduce.tera`, repeat until one remains
5. **Output** — Print narrative to stdout, optionally write JSON audit trail

## Definition of Done

- `git-chronicle` runs against a real repo and produces readable prose
- All CLI flags are implemented and validated (`<path>`, `--backend`, `--model`, `--group-size`, `--no-diffs`, `--template`, `--output`, `--author`, `--since`, `--branch`, `--from`, `--to`)
- JSON audit trail is complete and machine-readable
- `batch.tera` and `reduce.tera` ship as built-in defaults, overridable via `--template`
- Progress bars show per-pass status
- Errors (LLM unreachable, bad repo path, invalid template) surface with clear messages

## Scope

**In scope:**
- Local git repos only
- Local Ollama instances (default) or Claude API (optional, via `--backend claude`)
- Commit messages + optional diffs as input
- Single-branch history per invocation

**Out of scope:**
- Pull request or issue summarisation
- Web UI or TUI
- Streaming LLM output to terminal
- Git blame or per-file attribution
- Multi-repo aggregation
- Windows support in v0.1
