# Authority Model

This document is the single place that answers **"who can change what, and how fast"** for the
contract. Privileged actions are reachable through several routes — the admin directly, governance
proposals, the timelock, and bridge relayers. Each of those routes is documented in isolation
(`docs/admin-rotation.md`, `docs/timelock.md`, and the governance/bridge modules), which makes it
impossible to reason about the contract's real security property without reading all of them.

The real security property is **the fastest path to each privileged action**, not the slowest. A
slow timelock is irrelevant if the same action can also be taken immediately by the admin. Every
privileged function below is listed exactly once, together with *all* of its routes.

## Roles

Privileged entrypoints are gated by a single, narrow role rather than one omnipotent `admin` key.
Roles are stored per member under `DataKey::RoleMember(Role, Address)` and managed through
`grant_role`, `revoke_role`, `renounce_role` and `has_role`. Only `Admin` may grant roles; in
production the role admin is the governor contract.

| Role | Held by | Notes |
| --- | --- | --- |
| `Admin` | EOA / multisig set at deploy, rotatable | Grants and revokes roles; the role admin is the governor in production |
| `Minter` | Backend signer | Authorized to mint |
| `Revoker` | Backend signer | Authorized to revoke |
| `Pauser` | Governor / safety operator | Authorized to pause and unpause |
| `Upgrader` | Governor | Authorized to upgrade the WASM |
| `Bridge` | Bridge relayer set | Authorized to relay bridge messages |

`grant_role` and `revoke_role` emit `role_granted` and `role_revoked` events. The last `Admin`
cannot renounce itself, which prevents bricking the contract.

## Privileged actions and their routes

### 1. `setAdmin` / admin rotation

- **Routes:** admin direct; governance proposal; timelock.
- **Who may initiate:** current `admin` (direct), `governance` (proposal), `timelock` (scheduled call).
- **Delay:** none for the admin direct path; governance proposal delay for the governance path;
  timelock delay for the timelock path.
- **Who can cancel:** the initiator before execution; a pending timelock operation can be cancelled
  by the timelock's canceller (admin/governance per `docs/timelock.md`).
- **Fastest path:** **admin direct — immediate.**
- **Differing guarantees:** yes. The admin path is instant and uncancellable by anyone else, while
  the governance and timelock paths are delayed and cancellable. See `docs/admin-rotation.md`.

### 2. `setTimelock` / timelock configuration

- **Routes:** admin direct; governance proposal; timelock.
- **Who may initiate:** `admin`, `governance`, or `timelock`.
- **Delay:** none (admin direct); governance proposal delay; timelock delay.
- **Who can cancel:** initiator before execution; pending timelock ops cancellable by the canceller.
- **Fastest path:** **admin direct — immediate.**
- **Differing guarantees:** yes. Changing the timelock through the admin bypasses the timelock's own
  delay, so the delay is only as strong as the admin path is restricted. See `docs/timelock.md`.

### 3. `setGovernance` / governance configuration

- **Routes:** admin direct; governance proposal; timelock.
- **Who may initiate:** `admin`, `governance`, or `timelock`.
- **Delay:** none (admin direct); governance proposal delay; timelock delay.
- **Who can cancel:** initiator before execution; pending timelock ops cancellable by the canceller.
- **Fastest path:** **admin direct — immediate.**
- **Differing guarantees:** yes. The admin can replace governance without a proposal, so governance
  control is not a hard guarantee while the admin path exists.

### 4. `setRelayer` / relayer set management

- **Routes:** admin direct; governance proposal; timelock.
- **Who may initiate:** `admin`, `governance`, or `timelock`.
- **Delay:** none (admin direct); governance proposal delay; timelock delay.
- **Who can cancel:** initiator before execution; pending timelock ops cancellable by the canceller.
- **Fastest path:** **admin direct — immediate.**
- **Differing guarantees:** yes. Adding or removing a relayer directly is instant, whereas the
  governance/timelock routes are delayed.

### 5. `pause` / `unpause`

- **Routes:** admin direct; governance proposal; timelock.
- **Who may initiate:** `admin`, `governance`, or `timelock`.
- **Delay:** none (admin direct); governance proposal delay; timelock delay.
- **Who can cancel:** initiator before execution; pending timelock ops cancellable by the canceller.
- **Fastest path:** **admin direct — immediate.**
- **Differing guarantees:** yes. Pausing is a safety action and is intentionally fast on the admin
  path; the delayed routes are slower but harder to abuse.

### 6. `upgrade` / implementation change

- **Routes:** admin direct; governance proposal; timelock.
- **Who may initiate:** `admin`, `governance`, or `timelock`.
- **Delay:** none (admin direct); governance proposal delay; timelock delay.
- **Who can cancel:** initiator before execution; pending timelock ops cancellable by the canceller.
- **Fastest path:** **admin direct — immediate.**
- **Differing guarantees:** yes. An upgrade can be executed instantly by the admin, so the timelock
  delay does not protect against a compromised admin.

### 7. `execute` / bridge message relay

- **Routes:** bridge relayer.
- **Who may initiate:** an authorized `relayer`.
- **Delay:** none — relayed messages execute on receipt.
- **Who can cancel:** no cancellation once a valid message is relayed; invalid messages are rejected
  by validation.
- **Fastest path:** **relayer — immediate.**
- **Differing guarantees:** no other route reaches this action, so its guarantees are uniform.

## Role-to-entrypoint matrix

Each privileged entrypoint requires exactly one role. This table is the source of truth that the
RBAC tests assert against (each role is proven unable to call another role's entrypoints).

| Entrypoint | Required role |
| --- | --- |
| `grant_role` | `Admin` |
| `revoke_role` | `Admin` |
| `renounce_role` | the caller's own role (last `Admin` cannot renounce) |
| `mint` | `Minter` |
| `revoke` | `Revoker` |
| `pause` / `unpause` | `Pauser` |
| `upgrade` | `Upgrader` |
| `execute` (bridge) | `Bridge` |

## Cross-route summary

| Privileged action | Admin direct | Governance | Timelock | Relayer | Fastest path |
| --- | --- | --- | --- | --- | --- |
| `setAdmin` | immediate | delayed | delayed | — | admin direct |
| `setTimelock` | immediate | delayed | delayed | — | admin direct |
| `setGovernance` | immediate | delayed | delayed | — | admin direct |
| `setRelayer` | immediate | delayed | delayed | — | admin direct |
| `pause` / `unpause` | immediate | delayed | delayed | — | admin direct |
| `upgrade` | immediate | delayed | delayed | — | admin direct |
| `execute` (bridge) | — | — | — | immediate | relayer |

## Actions reachable by more than one route with different guarantees

Every admin-gated action above is reachable by **three** routes with materially different guarantees:

- **Admin direct** — no delay, no external cancellation. This is the fastest path and therefore the
  contract's effective security property for that action.
- **Governance proposal** — delayed by the proposal lifecycle and cancellable while pending.
- **Timelock** — delayed by the timelock delay and cancellable by the timelock canceller.

Because the admin path is always the fastest, the delays on the governance and timelock routes only
provide protection to the extent that the admin key is itself protected (e.g. a multisig). Any
hardening of the authority model must start by constraining the admin direct path.

The bridge `execute` action is the only privileged action with a single route, so it has no
conflicting guarantees.

## Related documents

- `docs/admin-rotation.md` — the admin direct path in detail.
- `docs/timelock.md` — the timelock route, its delay, and its cancellation rules.
