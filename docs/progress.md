# Progress — git-chronicle

## What Works
- Documentation complete: project brief, product context, tech context, system patterns, implementation plan

## What's Left
- [x] Step 1: Project scaffold
- [x] Step 2: Error types
- [x] Step 3: Core data types
- [x] Step 4: CLI parsing
- [x] Step 5: Ingester
- [ ] Step 6: Batcher
- [ ] Step 7: Templates
- [ ] Step 8: LLM trait and MockBackend
- [ ] Step 9: OllamaBackend
- [ ] Step 10: ClaudeBackend
- [ ] Step 11: Reducer
- [ ] Step 12: Audit trail
- [ ] Step 13: Main pipeline
- [ ] Step 14: End-to-end validation

## Current Status
Step 5 complete — `ingest()` with `Filters` (author, since, SHA range, branch). Diffs generated per-commit and immediately dropped. 37 tests pass, no warnings.
