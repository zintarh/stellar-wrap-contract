# Security Recommendations

This document tracks security recommendations and the current mainnet readiness
status. The status below is **derived from open `security`-labelled issues** and
must not be hand-maintained. If any `security`-labelled issue is open, the
project is **NOT READY FOR MAINNET**.

## Overall Security Status

> **Status: ❌ NOT READY FOR MAINNET — BLOCKED BY OPEN SECURITY FINDINGS**
>
> This status is derived from the open `security`-labelled issues in the tracker.
> It cannot be marked ready while any such issue remains open. Do not cite this
document as evidence of readiness until the blocking findings below are closed.

### How this status is derived

The status is computed from the repository issue tracker, not maintained by hand:

- Query all open issues carrying the `security` label.
- If the result set is **non-empty**, the status is **NOT READY FOR MAINNET**.
- Only when **zero** open `security`-labelled issues remain may the status be
  changed to ready, and that change must reference the empty query result.

Example derivation (run against the tracker):

```
open security issues = issues where state == open AND labels contains "security"
if count(open security issues) > 0:
    status = "NOT READY FOR MAINNET"
else:
    status = "READY FOR MAINNET"
```

Because the status is a function of the open `security` label, it cannot drift
out of sync with reality while security issues remain open.

## Blocking Findings

This section is **checked as part of the release process**. Every entry must be
closed before a mainnet release is approved. The list mirrors the open
`security`-labelled issues.

- [ ] **#647 — Arbitrary contract invocation.** A caller can invoke arbitrary
  contracts, allowing unintended cross-contract calls.
- [ ] **#650 — Missing Merkle domain separation.** Merkle proofs lack domain
  separation, enabling cross-context proof reuse.
- [ ] **#651 — Unchecked arithmetic with overflow checks disabled.** Arithmetic
  in a profile with overflow checks disabled can silently wrap.
- [ ] **#653 — Mint signatures that never expire.** Mint signatures remain valid
  indefinitely, so a leaked signature can be replayed forever.
- [ ] **#672 — Timelock actions that stay executable forever.** Timelock actions
  never expire and remain executable indefinitely.

### Release-process gate

Before approving any mainnet release:

1. Re-run the derivation above against the tracker.
2. Confirm the open `security`-labelled issue count is **zero**.
3. Confirm every blocking finding above is checked off and closed.

If any step fails, the release is blocked.

## Recommendations

- Keep the `security` label applied to every security finding so the derived
  status stays accurate.
- Re-evaluate this document whenever a `security`-labelled issue is opened or
  closed.
