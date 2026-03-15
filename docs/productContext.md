# Product Context — git-chronicle

## Problem

Git history is technically rich but narratively useless.

`git log` gives you a reverse-chronological list of atomic changes. `git shortlog` gives you per-author counts. Changelogs give you curated bullet points written at release time. None of these answer the questions developers actually ask:

- *What has this project been doing for the last six months?*
- *Who drove the authentication rewrite — and what was the shape of their work?*
- *How did this codebase evolve from prototype to production?*
- *I'm new to this repo — what's the story so far?*

## Why Existing Tools Fall Short

Similar tools (GitGossip, Changeish, various GPT wrappers) perform single-pass summarisation: they send a window of commits to an LLM and get back a paragraph. This breaks down over large histories — context windows overflow, signal gets diluted, and output reads like a bullet list with conjunctions.

None implement a hierarchical reduce approach, support per-level prompt templates, or produce a structured audit trail of the summarisation process.

## Why Local-First

Cloud-based summarisation tools have two problems:

1. **Privacy** — Commit messages and diffs often contain internal context, customer references, or architectural decisions that should not leave the machine.
2. **Cost and friction** — API keys, rate limits, and per-token costs create a barrier that makes the tool expensive to run against a large repo.

Local LLMs via Ollama solve both by default. The cost is electricity. The data never leaves the machine.

For users who want higher-quality output and are comfortable with the trade-offs, `git-chronicle` optionally supports the Claude API. This is an explicit opt-in — the default remains fully offline.

## Who It's For

- **Developers onboarding** to an unfamiliar codebase — a narrative gives them the arc without reading 500 commit messages
- **Teams doing retrospectives or handoffs** — automatic reconstruction of history from actual commits, not from memory
- **Open source maintainers** — a draft project narrative generated from the commit record, not from what was remembered to document

## What the Output Should Feel Like

The narrative should read like a knowledgeable colleague explaining what happened — prose, not bullet points, not a changelog. Precise enough for the developer who lived through the history; accessible enough for a non-technical stakeholder.

## What It Is Not

- Not a replacement for changelogs — it complements them. Changelogs are curated; this is synthesised.
- Not authoritative. It is an LLM-generated interpretation. The audit trail exists so claims can be verified against source summaries.
- Not a code quality tool. It describes; it does not evaluate.
