## 👷 How to Start this Issue

**Step 1: Setup**
1.  **Fork** the repository to your own GitHub account.
2.  **Clone** your fork locally.
3.  Create a new **Branch** for this specific issue (e.g., `feat/mint-logic` or `ci/setup-actions`).

**Step 1a: Dev Container (optional)**
To skip local toolchain installs, open the repository in VS Code (Dev Containers extension) or GitHub Codespaces and choose **Reopen in Container**. The container provides the pinned Rust toolchain (`rust-toolchain.toml`), the `wasm32-unknown-unknown` target, Stellar CLI, cargo-mutants, cargo-fuzz, Node 20 and `wasm-opt`. On creation it runs `make build && make test` once. Run `make test` again afterwards to re-check your changes.

**Step 2: Standards**
* **Clean Commits:** Use descriptive commit messages (e.g., `feat: implement mint function` not `fix`).
* **No Force Pushing:** If you need to change something, add a new commit or squash locally before pushing.
* **Code Style:** Ensure `cargo fmt` and `cargo clippy` pass before submitting.
* **Doc Check:** Run `make doc` (or `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`) to verify documentation compiles without warnings.

### Markdown Formatting Decision

The repository does not enforce a Markdown formatter or linter. Markdown is reviewed manually to avoid broad formatting churn in tables, diagrams, and documentation examples. When editing Markdown:

- Use ATX headings (`#`, `##`, etc.) and leave a blank line around headings, lists, tables, and fenced code blocks.
- Close every fenced code block and add a language identifier when the block contains code.
- Use explicit `<br>` tags for intentional hard line breaks; do not leave trailing spaces.
- Preserve a final newline and keep prose readable without reflowing tables, command output, or protocol examples.

Before submitting documentation changes, review the rendered Markdown and check that fences are balanced, links resolve, and no accidental trailing whitespace was introduced. These checks are manual and are not currently part of CI.

### Test Authorization Policy

`Env::mock_all_auths()` accepts every `require_auth` call, so a test that uses it still passes when an entrypoint authorises the wrong address. Auth bugs are the most common Soroban vulnerability class, so this hides the bugs we most need to catch.

- **Write-path tests** (any entrypoint that mutates state) must scope auth exactly: `env.mock_auths(&[MockAuth { address, invoke }])` or `client.mock_auths(...)` for the one call under test. Alternatively, call `mock_all_auths()` for setup, then assert on `env.auths()` after the call and check the `(address, function)` tree matches the intended signer.
- **Read-only tests and setup** (`initialize`, fixture funding, read entrypoints) may keep `mock_all_auths()`.
- `make check-test-auth` (`scripts/check_mock_all_auths.py`) fails if the number of `mock_all_auths()` call sites grows. When you convert a file to exact auth mocks, lower `MAX_ALLOWED` in the script to the new total in the same PR.

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
- `./scripts/check_wasm_size.sh` — WASM size within the budget in `.github/wasm-size-limit` (see "Registry budgets" under Step 2.4)
- `docker build -t stellar-wrap-contract .` — Docker build works
- `cargo audit` — no security advisories at or above medium severity
- `cargo deny check` — licenses, duplicate crates, and advisories match `deny.toml` (exceptions are documented there)
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
* **New error condition introduced**: If a function can now return a new error code or a new `None` case, document this in `README.md` and ensure `docs/errors.md` is current.
* **Contributor workflow change**: If `CONTRIBUTING.md` describes a process that changes (e.g., a new CI check is added, a new hook is configured), update `CONTRIBUTING.md` to reflect the new workflow.

**Example**: When adding a test that confirms `get_wrap` returns `None` before initialization (Issue #244), the "Read methods" documentation in `README.md` was updated to document this behavior for client developers.

**Step 2.4: Where New Features Belong (v2 Workspace)**

The workspace is split so the registry stays small. Before writing code, decide which component owns the feature by walking this tree in order and stopping at the first "yes":

```text
Does it change soulbound wrap record state?
(mint, mint_wrap, transfer_wrap, burn_wrap, lifecycle transitions,
 DataKey::Wrap / WrapCount / LatestPeriod)
├── yes → REGISTRY: src/ (the root stellar_wrap_contract crate)
└── no
    Is it optional or economic?
    (staking, fees, rewards, opt-in add-ons, token movement that
     does not touch wrap records)
    ├── yes → SATELLITE: contracts/staking/ or proxy/
    └── no
        Is it aggregation?
        (indexing, rollups, cross-record or cross-event summaries,
         reconciliation of on-chain events)
        ├── yes → PIPELINE: pipeline/
        └── no → Is it presentation only?
                 ├── yes → FRONTEND: frontend/
                 └── no → Open an issue to discuss before coding.
```

Rules of thumb:

- Registry code must not gain behaviour that a satellite or the pipeline can provide from the outside (events, read-only queries, or a separate contract that calls the registry).
- A satellite may call the registry, but the registry must not depend on a satellite.
- Aggregation belongs off-chain. The pipeline reads events; it does not add on-chain storage for derived totals.

**Registry budgets**

Any PR that touches `src/` must stay within these limits:

- **WASM size:** the release build must be within the byte limit in `.github/wasm-size-limit` (currently `225000`). `scripts/check_wasm_size.sh` enforces this in CI. Do not raise the limit in the same PR as a feature; open a separate PR that justifies it.
- **Entrypoints:** the registry exposes only record-state operations and their reads. Each new entrypoint needs the justification described in the PR template and an explicit row in the pause-coverage table.
- **Instructions and memory:** the repository does not pin a per-call instruction or memory budget. For any new or changed registry entrypoint, include the simulated resource usage (instructions, read/write bytes, and fee estimate from `simulateTransaction` or `stellar contract invoke` simulation) for the old and new code in the PR description.
- **Storage:** each new persistent key needs a `DataKey` variant, a TTL decision, and a row in `STORAGE.md`.
- **Dependencies:** do not add a crate to the registry's `[dependencies]` unless the PR explains why the logic cannot live in a satellite or the pipeline.

**Step 3: Submission**
* Open a **Pull Request (PR)** to the `main` branch of the upstream repository.
* Link this Issue in your PR description (e.g., "Closes #1").
* Wait for code review and address any feedback.
