# Stellar Wrap v2 — Architecture RFC

Status: **Draft for maintainer sign-off** (epic #964)

This RFC describes a v2 layout in which the soulbound registry is small and
auditable, and every optional subsystem lives in its own contract that talks to
the registry through explicit roles. It is documentation only; no contract code
changes are proposed here.

## Goals

- Keep the core mint/revoke/burn path small enough to audit in isolation.
- Stop one bug in bridge/staking/governance from corrupting registry storage.
- Fit the WASM size budget (`.github/wasm-size-limit`) and move toward the
  <50 KB goal in #525.
- Allow upgrading one subsystem without redeploying all of them.

## Target contracts

| Contract | Responsibility |
| --- | --- |
| `wrap-registry` | Mint, revoke, burn, queries, TTL management. Owns the soulbound token state. |
| `wrap-governor` | Proposals + timelock. Holds the registry admin role. |
| `wrap-bridge` | Cross-chain in/out. Holds the registry bridge/minter role. |
| `wrap-fees` | Fee parameters + staking. Reads registry state, never writes it. |
| `wrap-types` (crate) | Shared types: `DataKey`-equivalent per-contract keys, role enums, DTOs. |
| `wrap-crypto` (crate) | Shared crypto: merkle whitelist, oracle verification, alias hashing. |

## Diagram

```
                    +-------------------+
                    |   wrap-governor   |
                    | proposals+timelock|
                    +---------+---------+
                              | admin role
                              v
  +-------------+   mint/revoke   +-------------------+
  | wrap-bridge |---------------->|   wrap-registry   |
  | cross-chain |  bridge role    | mint/revoke/burn  |
  +-------------+                 | queries + TTL     |
                                  +---------+---------+
                                            ^ read-only
                                            |
                                  +---------+---------+
                                  |    wrap-fees      |
                                  | fee params+stake  |
                                  +-------------------+

  shared crates:  wrap-types  <-- all contracts
                  wrap-crypto <-- registry, bridge, fees
```

## Trust boundaries

Each registry entrypoint is callable only by the listed role. Roles are stored
in the registry and granted/revoked by the admin (held by `wrap-governor`).

| Registry entrypoint | Allowed caller | Role |
| --- | --- | --- |
| `mint` | `wrap-bridge`, `wrap-governor` | `Minter` |
| `revoke` | `wrap-governor` | `Revoker` |
| `burn` | token owner (self) | — |
| `transfer` | disabled (soulbound) | — |
| `set_ttl` / `extend_ttl` | `wrap-registry` internal, `wrap-governor` | `Admin` |
| `grant_role` / `revoke_role` | `wrap-governor` | `Admin` |
| queries (`balance`, `owner_of`, `is_valid`, ...) | anyone | — |
| `set_fee_params` | `wrap-fees` | `FeeManager` |
| `verify_oracle` | `wrap-bridge` | `Oracle` |
| `set_whitelist_root` | `wrap-governor` | `Admin` |

`wrap-fees` and any read-only consumer must never hold a write role on the
registry.

## Entrypoint mapping

Every current `pub fn` in `src/lib.rs` is mapped below to its v2 home, or marked
**deprecated** for removal.

### wrap-registry

- `initialize`, `mint`, `revoke`, `burn`, `transfer` (soulbound no-op)
- `balance`, `owner_of`, `is_valid`, `total_supply`, `token_uri`
- `set_ttl`, `extend_ttl`, `get_ttl`
- `grant_role`, `revoke_role`, `has_role`
- `set_whitelist_root`, `is_whitelisted` (uses `wrap-crypto`)
- `set_alias`, `resolve_alias`, `opt_out`, `opt_in`

### wrap-governor

- `propose`, `vote`, `execute`, `cancel`, `get_proposal`
- `queue_timelock`, `execute_timelock`, `cancel_timelock`
- `set_admin` (registry admin handoff)

### wrap-bridge

- `bridge_in`, `bridge_out`, `verify_oracle`, `set_oracle`

### wrap-fees

- `set_fee_params`, `get_fee_params`, `stake`, `unstake`, `claim_rewards`

### Deprecated / removed

- `set_fee` (superseded by `wrap-fees::set_fee_params`)
- `admin_mint` (superseded by role-gated `mint`)
- `emergency_pause` (moved to `wrap-governor` timelock)
- Any legacy alias helpers replaced by `wrap-crypto`

## Migration path

See the migration child issue. High level:

1. Deploy `wrap-registry` v2 and grant `wrap-governor` the admin role.
2. Deploy `wrap-governor`, `wrap-bridge`, `wrap-fees` and grant their roles.
3. Snapshot v1 state and replay mints/revocations into v2 registry.
4. Freeze v1 writes, point clients at v2 addresses, retire v1.

## Child issues

- [ ] Migration from deployed v1 instance
- [ ] `wrap-registry` extraction
- [ ] `wrap-governor` extraction
- [ ] `wrap-bridge` extraction
- [ ] `wrap-fees` extraction
- [ ] `wrap-types` / `wrap-crypto` shared crates

## Sign-off

Maintainers must approve this RFC before implementation PRs land.
