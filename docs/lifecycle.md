# Wrap Lifecycle

This document is the source of truth for the wrap lifecycle state machine
(`WrapState` / `WrapLifecycleFSM` in `src/storage_types.rs`). The Rust
`can_transition_to` match and the property test in `src/storage_types.rs` are
expected to agree with the transition table below.

## States

| State     | Meaning                                                        |
|-----------|----------------------------------------------------------------|
| `Active`  | A wrap that exists on this chain and can be revoked or burned. |
| `Bridged` | A wrap that has been bridged out to another chain.             |
| `Expired` | A wrap whose expiration duration has elapsed.                  |

Every state is reachable from at least one non-admin product flow:

- `Active` — created by `mint_wrap` / `mint_wrap_batch`.
- `Bridged` — reached by `bridge_wrap_out`.
- `Expired` — reached by `expire_wrap` once the expiration duration has
  elapsed (works on a freshly minted `Active` record).

`Draft`, `Pending`, `Archived` and `Cancelled` were removed: they had no
product flow and only existed behind the admin-only `transition_wrap_state`
escape hatch.

## Transitions

| From      | To        | Trigger            |
|-----------|-----------|--------------------|
| `Active`  | `Bridged` | `bridge_wrap_out`  |
| `Bridged` | `Active`  | `bridge_wrap_in`   |
| `Active`  | `Expired` | `expire_wrap`      |
| `Bridged` | `Expired` | `expire_wrap`      |

Self-transitions are not allowed. Any edge not listed above is rejected by
`WrapLifecycleFSM::can_transition_to`.

## Revoke and burn

Revoke and burn are **deletions with events**, not FSM states. They remove the
wrap record and emit an event; they do not move the record into a terminal
state. This keeps the state set minimal and avoids unreachable terminal states.

## Admin transitions

`transition_wrap_state` is restricted to the documented edges above. It cannot
introduce states or edges that are not in this table.
