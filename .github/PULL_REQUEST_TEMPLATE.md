## Summary

<!-- What does this PR change, and why? -->

Closes #

## Checklist

- [ ] The PR title references the issue number (e.g., `feat: add X (#123)`).
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` pass.
- [ ] `cargo test` passes and new or modified behaviour is covered by tests.
- [ ] If the PR changes contributor-facing workflow, `CONTRIBUTING.md` is updated.
- [ ] If the PR adds or changes a public function, `README.md` and `ERRORS.md` are updated.
- [ ] Any new state-mutating entrypoint in `src/lib.rs` is listed in `src/pause_coverage_test.rs`.
- [ ] Feature placement was checked against the decision tree in `CONTRIBUTING.md` (Step 2.4).
- [ ] Does this add a registry entrypoint? Justify. (If yes, explain in the PR description why the logic cannot live in a satellite contract or the pipeline, and include simulated resource usage before and after.)
- [ ] If this touches `src/`, the WASM size is within `.github/wasm-size-limit`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
