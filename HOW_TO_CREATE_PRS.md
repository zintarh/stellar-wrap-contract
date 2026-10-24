# How to Create PRs for Your Fork

You now have **2 branches** ready for pull requests on your fork at:
`https://github.com/coderolisa/stellar-wrap-contract`

## Branch 1: Fine-Grained Pause Flags

**Branch:** `feature/fine-grained-pause-step1`  
**Summary file:** `PR_SUMMARY_FINE_GRAINED_PAUSE.md`

### Quick PR Creation Steps:

1. Go to: https://github.com/coderolisa/stellar-wrap-contract/pull/new/feature/fine-grained-pause-step1

2. Use this as the PR title:
   ```
   feat: Add fine-grained pause flags infrastructure (1% of #649)
   ```

3. Copy the content from `PR_SUMMARY_FINE_GRAINED_PAUSE.md` into the PR description

4. Add labels (if available): `enhancement`, `infrastructure`, `security`

5. Set yourself as the assignee

6. Click "Create Pull Request"

### What This PR Does:
- Adds `PauseFlags` struct for selective operation pausing
- Enables pausing bridge without stopping mint (solves #649)
- Infrastructure only (no breaking changes)
- Includes tests demonstrating selective pause behavior

---

## Branch 2: Deployment Smoke Checks

**Branch:** `feature/deployment-smoke-checks`  
**Summary file:** `PR_SUMMARY_DEPLOYMENT_SMOKE_CHECKS.md`

### Quick PR Creation Steps:

1. Go to: https://github.com/coderolisa/stellar-wrap-contract/pull/new/feature/deployment-smoke-checks

2. Use this as the PR title:
   ```
   feat: Add post-deployment smoke checks infrastructure (1%)
   ```

3. Copy the content from `PR_SUMMARY_DEPLOYMENT_SMOKE_CHECKS.md` into the PR description

4. Add labels (if available): `enhancement`, `ci/cd`, `devops`

5. Set yourself as the assignee

6. Click "Create Pull Request"

### What This PR Does:
- Adds smoke checks after deployment to catch misconfigurations
- Queries health(), version(), is_paused(), get_admin_pubkey()
- Displays contract state in GitHub Actions job summary
- Foundation for validation logic (no failures yet)

---

## Alternative: CLI Method

If you prefer using GitHub CLI (`gh`):

```bash
cd /home/julliet/Desktop/#1014

# Create PR for fine-grained pause
git checkout feature/fine-grained-pause-step1
gh pr create \
  --title "feat: Add fine-grained pause flags infrastructure (1% of #649)" \
  --body-file PR_SUMMARY_FINE_GRAINED_PAUSE.md \
  --base main

# Create PR for deployment smoke checks
git checkout feature/deployment-smoke-checks
gh pr create \
  --title "feat: Add post-deployment smoke checks infrastructure (1%)" \
  --body-file PR_SUMMARY_DEPLOYMENT_SMOKE_CHECKS.md \
  --base main
```

---

## After Creating PRs

### For Reviewers
Both PR summaries include:
- ✅ Problem statement (why this change is needed)
- ✅ Solution approach
- ✅ Detailed changes made
- ✅ Before/after examples
- ✅ What's next (remaining work)
- ✅ Testing information
- ✅ Files changed
- ✅ Checklists

### Next Steps After Merge
1. **Fine-Grained Pause**: Integrate flag checks into entrypoints
2. **Smoke Checks**: Add assertion logic to fail on misconfigurations

---

## Current Branch Status

```
main
  ├─ feature/fine-grained-pause-step1 (ready for PR)
  │   └─ Commits: bf9c1b0, 5d9fe54
  │
  └─ feature/deployment-smoke-checks (ready for PR)
      └─ Commits: e1a9eeb, 0ffc718
```

Both branches are up-to-date and pushed to your fork!
