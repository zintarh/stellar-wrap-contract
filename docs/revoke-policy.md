# `revoke_wrap` Operational Policy

`revoke_wrap` permanently deletes an existing wrap record on-chain. It is
admin-only, irreversible for that specific record, and — unlike minting —
requires no signature from an off-chain signing key, only
`admin.require_auth()` (see [`src/revoke.rs`](../src/revoke.rs)). Because a
single authorized call can erase a user's wrap with no on-chain trace of
*why*, this document defines when revocation is appropriate and what must
happen around it operationally.

---

## When revocation is appropriate

Revoke a wrap only for one of these reasons:

1. **Fraud or abuse** — the wrap was minted for a user/period as part of a
   confirmed fraudulent claim (e.g. a forged or replayed off-chain
   authorization, a chargeback, or a violation of program terms).
2. **Backend/off-chain error** — the signing service minted the wrap with
   incorrect data (wrong `archetype`, wrong `data_hash`, wrong `period`) due
   to a bug or bad input, and the record needs to be cleared so the correct
   claim can be reminted.
3. **Compliance or legal request** — a legitimate takedown/compliance
   requirement (e.g. sanctions, court order) mandates removing the record.

There is no separate "correct the metadata in place" entrypoint — the
contract has no method to change an existing wrap's `archetype` or
`data_hash`. Revoke-and-remint (see
[Reminting after revocation](#reminting-after-revocation)) is the only
supported way to fix a wrap minted with wrong data, which is exactly why
"backend error" is a first-class, expected revocation reason rather than an
edge case.

---

## Off-chain evidence must exist before you revoke

`revoke_wrap` takes a `reason_hash: BytesN<32>` parameter — a one-way SHA-256
hash of an off-chain revocation reason (see the doc comment on
`revoke_wrap` in `src/revoke.rs`). The hash alone proves nothing after the
fact unless the original evidence is retained, so:

- **Write down and store the actual reason *before* calling `revoke_wrap`.**
  Keep it in a durable, verifiable location — a signed document, an internal
  ticket/governance log, or content-addressed storage (e.g. IPFS) — so that
  `sha256(evidence) == reason_hash` can be recomputed and checked by an
  auditor later.
- Never call `revoke_wrap` with only a hash and no retrievable evidence
  behind it. If you have no reason to record, you likely don't have a valid
  reason to revoke (see [When revocation is appropriate](#when-revocation-is-appropriate)).
  Passing an all-zero `BytesN<32>` is only meant for the (discouraged) case
  where no reason is being disclosed at all — the `revoke` event still fires
  for transparency, but it can't be tied to any evidence.
- Retain the evidence for at least as long as your compliance/audit
  requirements demand, independent of the chain's own retention.

## Recommended operational checklist

Before invoking `revoke_wrap(user, period, reason_hash)`:

1. Confirm the wrap exists: `get_wrap(user, period)` — `revoke_wrap` panics
   with `Error(Contract, #7)` (`WrapNotFound`) otherwise.
2. Confirm the revocation reason falls under one of the categories in
   [When revocation is appropriate](#when-revocation-is-appropriate).
3. Write the off-chain evidence to your durable store first, then compute
   `reason_hash = sha256(evidence)`.
4. Call `revoke_wrap(user, period, reason_hash)` with admin authorization.
5. Confirm the emitted `revoke` event (topics `["revoke", user, period]`,
   data `reason_hash`) and cross-reference it with the stored evidence in
   your audit log.
6. If the user is owed a corrected wrap, remint per
   [Reminting after revocation](#reminting-after-revocation) below.

---

## What revocation actually does on-chain

`revoke_wrap` (in `src/revoke.rs`):

- Requires the stored admin's authorization (`admin.require_auth()`).
- Panics with `Error(Contract, #7)` (`WrapNotFound`) if no wrap exists for
  `(user, period)`.
- **Fully deletes** the wrap record (`storage().persistent().remove`), not
  just marks it revoked — there is no `WrapLifecycleFSM` "revoked" state left
  behind for that key.
- Decrements the user's wrap count (`WrapCount`) and, if this was the user's
  latest period, clears `LatestPeriod`.
- Increments the contract-wide `TotalRevoked` counter, queryable via
  `total_revoked()`.
- Emits a `revoke` event with topics `["revoke", user, period]` and data
  `reason_hash`, so indexers can reconstruct revocation history even though
  the on-chain record itself is gone.

## Reminting after revocation

Because revocation removes the `(user, period)` storage entry entirely
rather than flagging it, **the same `(user, period)` pair can be reminted**
once the wrap is revoked — `mint_wrap`'s duplicate check
(`Error(Contract, #4)`, `WrapAlreadyExists`) only triggers while a record
still exists for that key. This is intentional, exercised by
`test_remint_after_revoke_updates_archetype` in `src/test.rs`, and is the
supported path for correcting a wrap after a backend error: revoke the bad
record, then mint a fresh one with the correct `archetype`/`data_hash` for
the same period.

Two consequences of this follow directly:

- A revoked-then-reminted period has **no on-chain link** between the old
  and new record — only the `revoke` and `mint` event history (and your
  off-chain audit trail) tie them together. Don't rely on `get_wrap` alone to
  reconstruct history for a period that was ever revoked.
- Because reminting is possible, revocation is not a way to permanently bar
  a user from ever holding a wrap for that period — it only clears the
  current record. Pair it with off-chain controls if a user must be
  permanently blocked.

---

## Error reference

| Code | Name | Triggered when |
|------|------|----------------|
| `#3` | `Unauthorized` | Caller is not the stored admin |
| `#7` | `WrapNotFound` | No wrap record exists for `(user, period)` |

See [errors.md](./errors.md) for the full error catalogue.
