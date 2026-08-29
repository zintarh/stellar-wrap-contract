### Grace Period and Expiration

Scheduled operations carry a mandatory grace period of **14 days** (`GRACE_PERIOD = 1,209,600` seconds).

- **Valid execution window:** `op.eta <= ledger.timestamp <= op.eta + GRACE_PERIOD`.
- **Expired operations:** If `ledger.timestamp > op.eta + GRACE_PERIOD`, calling `timelock_execute` will panic with `TimelockOperationExpired` (66).
- **Sweeping expired operations:** Anyone can call `timelock_sweep_expired(id)` once an operation is expired. This removes the operation from storage and the `TimelockOps` list so `timelock_pending` stays truthful.

## What the timelock closes off

Once enabled, the direct paths panic with `TimelockRequired` (44).

## Privileged entrypoint coverage

Once enabled, entrypoints marked **Yes** reject direct calls with
`TimelockRequired` and must be reached through `timelock_schedule` plus
`timelock_execute`. Entry points marked **No** remain direct admin calls by
design and are not represented by a `TimelockAction` variant.

| Entrypoint | Timelocked? | Rationale |
| --- | --- | --- |
| `initialize` | No | Deployment bootstrap is single-use and must be signed by the configured admin account. |
| `update_admin` | Yes | Ownership changes need an observable delay. |
| `propose_admin` / `accept_admin` | Yes | Two-step handover must not bypass the delay. |
| `cancel_proposed_admin` | No | Clearing a stale handover does not grant access or change ownership. |
| `upgrade` | Yes | WASM changes need an observable delay. |
| `set_whitelist_root` / `clear_whitelist_root` | Yes | Whitelist access-control changes need an observable delay. |
| `TimelockAction::SetAdminPubKey` | Yes | Mint-signing key rotation needs an observable delay; it has no direct entrypoint. |
| `migrate` | No | No corresponding timelock action exists; retained as a direct admin migration operation. |
| `set_name` / `set_symbol` | No | No corresponding timelock action exists; retained as direct admin metadata configuration. |
| `backfill_wrap_periods` | No | No corresponding timelock action exists; retained as a direct admin migration operation. |
| `pause` / `unpause` | No | An emergency stop must remain immediately available. |
| `set_transfer_fee` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_expiration_duration` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_fee_params` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_stake_config` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_bridge_relayer` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_chain_status` | No | No corresponding timelock action exists; retained as a direct admin configuration. |