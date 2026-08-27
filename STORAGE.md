# Storage accounting and algorithmic fee

This contract now keeps a conservative estimated count of persistent storage bytes in instance storage and exposes an on-chain algorithmic fee computed from that estimate.

Key details:

- DataKey::StorageBytes (instance) stores a u64 estimate of persistent bytes.
- DataKey::FeeParams (instance) stores a FeeParams struct with these fields:
  - base_fee: i128
  - per_kib_fee: i128
  - scale_step_kib: u64
  - max_fee: i128

Fee formula (on-chain):

fee = min(max_fee, base_fee + per_kib_fee * ceil(storage_bytes / 1024 / scale_step_kib))

Accounting updates:

- mint_wrap() increments estimate when creating new persistent entries (wrap record, wrap count, latest, user_periods) using conservative constants.
- revoke_wrap() decrements estimate for the removed wrap and for the wrap-count entry when it reaches zero.

---

### Temporary storage (`e.storage().temporary()`)
**What lives here**

Non-critical global state that has sensible defaults when absent:
- `DataKey::TotalRevoked` → `u64` (default: `0`)
- `DataKey::Paused` → `bool` (default: unpaused)
- `DataKey::Name` → `String` (default: `"Stellar Wrap Registry"`)
- `DataKey::Symbol` → `String` (default: `"WRAP"`)

**TTL behavior**
- Temporary storage entries are extended to **~1 day** (17,280 ledgers) on every write.
- When entries expire, the contract gracefully falls back to hardcoded defaults.
- This dramatically reduces state rent fees compared to storing in Instance.

**Why temporary storage?**
- These entries are non-critical: the contract functions correctly with or without them.
- Temporary storage has the lowest state rent cost of all three tiers.
- If an admin customizes `Name`/`Symbol` and the entry expires, it simply reverts to the default.
- `TotalRevoked` is an informational counter that can be derived from revoke events if lost.

---

### Persistent storage (`e.storage().persistent()`)
**What lives here**

For each user:
- `DataKey::Wrap(Address, u64)` → `WrapRecord`
  - key space: `(user, period)`
- `DataKey::WrapCount(Address)` → `u32`
  - total wraps minted for the user
- `DataKey::LatestPeriod(Address)` → `u64`
  - maximum `period` value minted so far

**TTL behavior**
- Every persistent entry is extended on write to **~1 year**.
- The contract uses the same TTL value:

```rust
let ttl_one_year = 17280 * 365; // ~1 year in ledgers
```

- On `mint_wrap()` and `update_wrap()` the contract does:
  - `set(key, value)`
  - `extend_ttl(key, ttl_one_year, ttl_one_year)`

**Why do this?**
- Soroban persistent entries can expire if TTL is not extended.
- A ~1 year TTL provides a long window where the entry remains accessible.
- The contract exposes `extend_ttl()` so entries can be renewed indefinitely.

**Cost implication**
- Persistent storage writes incur higher cost than instance storage.
- TTL extension is an additional operation that is performed only when the entry is written/updated or explicitly renewed.

---

### MintGuard (temporary storage)

- `DataKey::MintGuard(Address)` → `bool` (stored as `true`)

This is a mint reentrancy / double-call guard.

The contract also removes the guard on successful completion:

```rust
e.storage().temporary().remove(&guard_key);
```

If execution panics before removal, the guard does not permanently block future mints—temporary storage will expire/clear.

---

## 2) DataKey enum → storage key mapping

The contract defines the following storage key enum with their tier assignments:

- `DataKey::Admin` (instance)
- `DataKey::AdminPubKey` (instance)
- `DataKey::PendingAdmin` (instance)
- `DataKey::MigrationVersion` (instance)
- `DataKey::Wrap(Address, u64)` (persistent)
- `DataKey::WrapCount(Address)` (persistent)
- `DataKey::LatestPeriod(Address)` (persistent)
- `DataKey::UserPeriods(Address)` (persistent)
- `DataKey::TotalWrapCount` (persistent)
- `DataKey::AliasHash(Address)` (persistent)
- `DataKey::TotalRevoked` (temporary)
- `DataKey::Paused` (temporary)
- `DataKey::Name` (temporary)
- `DataKey::Symbol` (temporary)
- `DataKey::MintGuard(Address)` (temporary — managed separately)

```rust
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    AdminPubKey,
    PendingAdmin,
    Wrap(Address, u64),
    WrapCount(Address),
    LatestPeriod(Address),
    MigrationVersion,
    UserPeriods(Address),
    Paused,
    TotalWrapCount,
    TotalRevoked,
    AliasHash(Address),
    Name,
    Symbol,
}
```

> **Note:** The `MintGuard` key is managed separately from the `DataKey` enum using ephemeral temporary storage keys scoped to each mint invocation.

> **Note on key serialization:** `DataKey` is annotated with `#[contracttype]`. Soroban serializes `contracttype` enums into a canonical storage-key representation (using enum discriminants/variants plus the associated values).

---

## 3) Key encoding scheme (Soroban contracttype)

For a storage operation like:

```rust
let wrap_key = DataKey::Wrap(user.clone(), period);
let exists = e.storage().persistent().has(&wrap_key);
```

Soroban converts `wrap_key` into a byte representation suitable for storage lookups.

### DataKey::Wrap(Address, u64)
Conceptually, the serialized key contains:

1. **Enum variant identifier** for `Wrap` (a discriminant derived from the enum variant ordering)
2. The encoded fields:
   - `Address` (user)
   - `u64` (period)

So the logical form is:

> **Key(wrap) = Encode(contracttype enum `DataKey` with variant `Wrap`, payload = (user, period))**

The exact byte layout is produced by Soroban’s `contracttype` serialization (and must be treated as canonical, deterministic encoding).

---

## 4) TTL strategy and `extend_ttl()` mechanics

### TTL value used
All persistent user entries (wrap records + count + latest period) are renewed to:

- `ttl_one_year = 17280 * 365`

### Where TTL is extended
- `mint_wrap()` extends TTL immediately after creating:
  - `DataKey::Wrap(user, period)`
  - `DataKey::WrapCount(user)`
  - `DataKey::LatestPeriod(user)` (only if it increases)

- `update_wrap()` extends TTL on the updated `DataKey::Wrap(user, period)`

- `extend_ttl(e, user, period)` renews TTL for the user’s entries if they already exist:
  - `Wrap(user, period)` (for the provided period argument)
  - `WrapCount(user)`
  - `LatestPeriod(user)`

```rust
pub fn extend_ttl(e: Env, user: Address, period: u64) {
    let wrap_key = DataKey::Wrap(user.clone(), period);
    let ttl = 17280 * 365;

    if e.storage().persistent().has(&wrap_key) {
        e.storage().persistent().extend_ttl(&wrap_key, ttl, ttl);
    }

    let count_key = DataKey::WrapCount(user.clone());
    if e.storage().persistent().has(&count_key) {
        e.storage().persistent().extend_ttl(&count_key, ttl, ttl);
    }

    let latest_key = DataKey::LatestPeriod(user);
    if e.storage().persistent().has(&latest_key) {
        e.storage().persistent().extend_ttl(&latest_key, ttl, ttl);
    }

    e.storage().instance().extend_ttl(ttl, ttl);
}
```

> **Implication:** If you want all wrap records for a user to remain accessible, you must call `extend_ttl()` for each `(user, period)` record (and/or call it with each existing period you care about). The function also renews the shared `WrapCount` and `LatestPeriod`.

### Cost implications (high level)
- **More wraps ⇒ more persistent entries ⇒ more writes and more storage footprint**.
- TTL extension is performed per entry; it does not “bulk extend” the entire wrap history unless you call it for each record.

---

## 5) MintGuard pattern (temporary storage)

### Purpose
During `mint_wrap()`, before any persistent writes happen, the contract creates a guard:

- `guard_key = DataKey::MintGuard(user)`
- If the guard exists already in temporary storage, the call panics with `Unauthorized`.
- On successful completion it removes the guard.

### Why it works
- Temporary storage entries do not persist permanently.
- If a mint fails (panic) before removal, the guard will naturally clear due to temporary TTL behavior.

---

## 6) Storage layout diagram for a user with N wraps

Assume a single user address `U` and that the user has `N` distinct wraps (periods):

- Periods: `p1, p2, ... pN`
- LatestPeriod is `max(pi)`

### Instance storage (shared)

```
Instance
- Admin
- AdminPubKey
- PendingAdmin
- MigrationVersion
```

### Temporary storage (during mint call)

During a `mint_wrap(U, period)` invocation:

```
Temporary (during call)
- MintGuard(U) = true

After successful mint:
- MintGuard(U) removed
```

### Persistent storage (steady-state)

```
Persistent (for user U)
- Wrap(U, p1)      -> WrapRecord
- Wrap(U, p2)      -> WrapRecord
...
- Wrap(U, pN)      -> WrapRecord

- WrapCount(U)    -> u32 (N)
- LatestPeriod(U) -> u64 (max(p1..pN))
```

**Total persistent entries per user:**
- `N` wraps + `1` wrap count + `1` latest period = **N + 2**

---

## 7) Storage size estimate and cost discussion

> **Important:** Exact Soroban storage byte costs depend on the chain’s current metering rules and the precise serialized size of values. This section provides *structural* size estimates so you can reason about scaling.

### Data sizes

#### WrapRecord
`WrapRecord` fields:
- `timestamp: u64` → 8 bytes
- `data_hash: BytesN<32>` → 32 bytes
- `archetype: Symbol` → variable length (typically small; serialized in Soroban)
- `period: u64` → 8 bytes
- `fsm: WrapLifecycleFSM` → struct containing `state: WrapState` (enum discriminant) and `updated_at: u64` (8 bytes)

**Baseline numeric bytes:**
- `8 + 32 + 8 + 8 = 56 bytes` + `Symbol & WrapState overhead`

#### Keys
Each persistent entry uses a `DataKey` key:
- Variant tag for `Wrap`
- `Address` bytes (Soroban Address is typically 32 bytes)
- `u64` period (8 bytes)

So key footprint scales with the number of wrap records.

### Per-wrap record footprint (rule-of-thumb)
For each wrap, you store:
- one persistent mapping entry `Wrap(U, period) -> WrapRecord`
- plus its key

So storage footprint scales approximately **O(N)**.

### Example totals per user
Let:
- `S_wrap` be approximate bytes for one `Wrap(U, period)` entry
- `S_count` be bytes for `WrapCount(U)`
- `S_latest` be bytes for `LatestPeriod(U)`

Then user storage is:

- Total ≈ `N * S_wrap + S_count + S_latest`

So:
- **100 wraps**: ≈ `100*S_wrap + const`
- **10,000 wraps**: ≈ `10,000*S_wrap + const`

**Scaling:** going from 100 → 10,000 increases wrap-entry storage by ~**100×**.

### What’s “const” here?
- `WrapCount(U)` and `LatestPeriod(U)` are only 2 persistent entries regardless of `N`.

---

## 8) Summary table of DataKey variants

| DataKey variant | Tier | Value type | TTL setting | Notes |
|---|---|---|---|---|
| `Admin` | instance | `Address` | shared instance lifecycle | set once during `initialize()` |
| `AdminPubKey` | instance | `BytesN<32>` | shared instance lifecycle | set once during `initialize()` |
| `PendingAdmin` | instance | `Address` | shared instance lifecycle | used for two-step admin transfer |
| `MigrationVersion` | instance | `u32` | shared instance lifecycle | tracks applied storage migrations |
| `Wrap(Address, u64)` | persistent | `WrapRecord` | `extend_ttl(..., ttl_one_year, ttl_one_year)` | exists per `(user, period)`; duplicated check uses `has()` |
| `WrapCount(Address)` | persistent | `u32` | `extend_ttl(..., ttl_one_year, ttl_one_year)` | incremented each `mint_wrap()` |
| `LatestPeriod(Address)` | persistent | `u64` | `extend_ttl(..., ttl_one_year, ttl_one_year)` only when updated | updated when `period` increases |
| `UserPeriods(Address)` | persistent | `Vec<u64>` | `extend_ttl(..., ttl_one_year, ttl_one_year)` | tracks all periods for a user |
| `TotalWrapCount` | persistent | `u32` | `extend_ttl(..., ttl_one_year, ttl_one_year)` | global count across all users |
| `AliasHash(Address)` | persistent | `BytesN<32>` | `extend_ttl(..., ttl_one_year, ttl_one_year)` | user-controlled alias hash |
| `TotalRevoked` | temporary | `u64` | `extend_ttl(..., ttl_temp, ttl_temp)` | global revoke counter; defaults to 0 |
| `Paused` | temporary | `bool` | `extend_ttl(..., ttl_temp, ttl_temp)` | contract pause flag; defaults to unpaused |
| `Name` | temporary | `String` | `extend_ttl(..., ttl_temp, ttl_temp)` | token name; defaults to "Stellar Wrap Registry" |
| `Symbol` | temporary | `String` | `extend_ttl(..., ttl_temp, ttl_temp)` | token symbol; defaults to "WRAP" |
| `MintGuard(Address)` | temporary | `bool` (stored as `true`) | auto-cleaned (temporary TTL) | removed explicitly on success |

---

## Appendix: Relevant contract locations

- Key definitions: `src/storage_types.rs` (`DataKey`, `WrapRecord`)
- Persistent writes + TTL extension: `src/lib.rs` (`mint_wrap`, `update_wrap`, `extend_ttl`)
- Temporary guard: `src/lib.rs` (`mint_wrap`)

- Because Soroban does not expose raw ledger storage size to contracts, this approach uses contract-side accounting and conservative estimates. To bootstrap existing deployments, use an admin migration that recomputes storage_bytes by scanning known indexes in bounded chunks (not provided here).

---

## 9) User State Invariants

The contract maintains a set of correlated storage entries for each user. To ensure storage consistency, the following invariants are defined and can be verified via the `check_user_invariants` entrypoint:

1. **WrapCount == UserPeriods.len()**: The total number of minted wraps must equal the number of periods tracked in `UserPeriods`.
2. **WrapCount == WrapPeriods.len()**: The total number of minted wraps must equal the number of periods tracked in `WrapPeriods`.
3. **LatestPeriod == max(UserPeriods)**: When a user has wraps, `LatestPeriod` must exactly match the maximum period in `UserPeriods`. If `UserPeriods` is empty, `LatestPeriod` must be absent.
4. **All periods live**: Every period recorded in `UserPeriods` must correspond to a live `Wrap` entry in persistent storage.
5. **balance_of == WrapCount**: The user's token balance (returned by `balance_of`) must exactly equal `WrapCount`.

These invariants must hold after any state transition, including minting, revoking, burning, or bridging wraps.
