## 👷 How to Start this Issue

**Step 1: Setup**
1.  **Fork** the repository to your own GitHub account.
2.  **Clone** your fork locally.
3.  Create a new **Branch** for this specific issue (e.g., `feat/mint-logic` or `ci/setup-actions`).

**Step 2: Standards**
* **Clean Commits:** Use descriptive commit messages (e.g., `feat: implement mint function` not `fix`).
* **No Force Pushing:** If you need to change something, add a new commit or squash locally before pushing.
* **Code Style:** Ensure `cargo fmt` and `cargo clippy` pass before submitting.
* **Doc Check:** Run `make doc` (or `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`) to verify documentation compiles without warnings.

**Step 2.1: Install Git Hooks (Required)**
1. Install pre-commit once on your machine:
	- `pip install pre-commit`
2. Install project hooks:
	- `pre-commit install`
	- `pre-commit install --hook-type pre-push`

Hook behavior in this repository:
- `pre-commit` stage runs `cargo fmt --check`
- `pre-push` stage runs `cargo clippy --all-targets -- -D warnings`

These hooks match CI enforcement:
- CI `Check format` step runs `cargo fmt --check` (same as pre-commit hook)
- CI `Run clippy` step runs `cargo clippy --all-targets -- -D warnings` (same as pre-push hook)

You can also run hooks manually:
- `pre-commit run --all-files`
- `pre-commit run --hook-stage pre-push --all-files`

**CI checks** (independent of pre-commit, run on every PR):
- `cargo test` — all unit and integration tests
- `cargo doc --no-deps` with `-D warnings` — documentation compiles
- `./scripts/check_wasm_size.sh` — WASM size within 200 KB budget
- `docker build -t stellar-wrap-contract .` — Docker build works
- `cargo audit` — no security advisories at or above medium severity
- `cargo tarpaulin --config tarpaulin.toml` — line coverage ≥ 90%
- `python3 scripts/check_readme_entrypoints.py` — all contract entrypoints documented in README

**Step 2.2: Pull Request Checklist**

Before opening a PR, confirm every item below:

- [ ] The PR title references the issue number (e.g., `feat: add X (#123)`).
- [ ] The PR description body contains `Closes #<issue-number>`.
- [ ] All new or modified behavior is covered by tests in `src/test.rs`.
- [ ] `cargo fmt --check` passes with no formatting differences.
- [ ] `cargo clippy --all-targets -- -D warnings` passes with zero warnings.
- [ ] `cargo test` passes and the full output is included in the PR description.
- [ ] Docker build passes (`docker build -t stellar-wrap-contract .`).
- [ ] Line coverage meets the ≥ 90% threshold (`cargo tarpaulin --config tarpaulin.toml`).
- [ ] If the PR adds or changes a public function, the "Read methods" or "Write methods" documentation in `README.md` is updated.
- [ ] If the PR changes contributor-facing workflow, `CONTRIBUTING.md` is updated.
- [ ] No `unwrap()` or `expect()` in production code paths (test code is exempt).
- [ ] All new public functions have `///` rustdoc comments.
- [ ] Any new state-mutating entrypoint added to `src/lib.rs` must appear in the pause-coverage table in `src/pause_coverage_test.rs`, with an explicit decision: blocked by `require_not_paused`, or intentionally allowed with a documented reason.

**Step 2.3: When to Update Documentation**

Documentation updates are required alongside code changes in the following cases:

* **New public function added**: Update the "Read methods" or "Write methods" section in `README.md` with the function's signature, return type, and behavior — including pre-initialization behavior if the function is callable before `initialize()` is called.
* **Existing public function behavior changed**: Update the corresponding documentation in `README.md` to reflect the new behavior, return values, or error conditions.
* **New error condition introduced**: If a function can now return a new error code or a new `None` case, document this in `README.md` and ensure `ERRORS.md` is current.
* **Contributor workflow change**: If `CONTRIBUTING.md` describes a process that changes (e.g., a new CI check is added, a new hook is configured), update `CONTRIBUTING.md` to reflect the new workflow.

**Example**: When adding a test that confirms `get_wrap` returns `None` before initialization (Issue #244), the "Read methods" documentation in `README.md` was updated to document this behavior for client developers.

**Step 3: Submission**
* Open a **Pull Request (PR)** to the `main` branch of the upstream repository.
* Link this Issue in your PR description (e.g., "Closes #1").
* Wait for code review and address any feedback.
