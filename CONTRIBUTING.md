## 👷 How to Start this Issue

**Step 1: Setup**
1.  **Fork** the repository to your own GitHub account.
2.  **Clone** your fork locally.
3.  Create a new **Branch** for this specific issue (e.g., `feat/mint-logic` or `ci/setup-actions`).

**Step 2: Standards**
* **Clean Commits:** Use descriptive commit messages (e.g., `feat: implement mint function` not `fix`).
* **No Force Pushing:** If you need to change something, add a new commit or squash locally before pushing.
* **Code Style:** Ensure `cargo fmt` and `cargo clippy` pass before submitting.

**Step 2.1: Install Git Hooks (Required)**
1. Install pre-commit once on your machine:
	- `pip install pre-commit`
2. Install project hooks:
	- `pre-commit install`
	- `pre-commit install --hook-type pre-push`

Hook behavior in this repository:
- `pre-commit` stage runs `cargo fmt --check`
- `pre-push` stage runs `cargo clippy --all-targets -- -D warnings`

You can also run hooks manually:
- `pre-commit run --all-files`
- `pre-commit run --hook-stage pre-push --all-files`

**Step 3: Submission**
* Open a **Pull Request (PR)** to the `main` branch of the upstream repository.
* Link this Issue in your PR description (e.g., "Closes #1").
* Wait for code review and address any feedback.
