# Stellar Wrap - Smart Contract

> **The on-chain Soulbound Token (SBT) registry for Stellar Wrap. This contract stores non-transferable wrap records linked to user addresses, containing data hashes and persona archetypes.**

This repository contains the **Soroban smart contract** that serves as the on-chain anchor for Stellar Wrap. For the full application (frontend, backend, etc.), see the main Stellar Wrap repository.

---

## 📖 What is Stellar Wrap?

Stellar Wrap is a "Spotify Wrapped"-style experience built specifically for the Stellar community.

Block explorers are great for data, but terrible for stories. Stellar Wrap takes your raw, complex on-chain history—transactions, smart contract deployments, NFT buys—and transforms it into a beautiful, personalized visual story that anyone can understand and share.

By simply connecting your wallet, you get a dynamic snapshot of your month on Stellar, highlighting your achievements and assigning you a unique on-chain persona based on your activity.

**It's more than just stats; it's a tool for builders to prove their contributions and for users to flex their participation in the Stellar ecosystem.**

--- 

## 💡 Why We Need This

In Web3, your on-chain history is your resume, your identity, and your reputation. But right now, that reputation is hidden behind confusing transaction hashes.

**Stellar Wrap solves the visibility gap:**

- **For Builders & Developers:** It's hard to showcase the immense value of deploying open-source Soroban contracts. Stellar Wrap makes their code contributions visible and shareable to non-technical users.
- **For the Community:** We lack easy, viral loops to share excitement about what's happening on Stellar. This tool gives everyone a reason to post about their on-chain life on social media.
- **For Users:** It turns isolated transactions into a sense of progress and belonging within the ecosystem.

---

## 🚀 How the Contract Works

This smart contract provides the on-chain registry for Stellar Wrap records:

1.  **Initialize:** The contract is initialized once with an admin address that has permission to mint wrap records.
2.  **Mint Wrap:** The admin (backend service) calls `mint_wrap()` to create a soulbound record for a user, storing:

- Timestamp of when the wrap was generated
- SHA256 hash of the full off-chain JSON data (ensuring integrity)
- Archetype/persona assigned to the user (e.g., _"soroban_architect"_, _"defi_patron"_, _"diamond_hand"_)

3.  **Query:** Anyone can call `get_wrap()` to retrieve a user's wrap record, enabling verification and display of on-chain personas.
4.  **Soulbound:** Records are non-transferable (SBT), permanently linked to the user's Stellar address.

---

## 🎯 Key Metrics Tracked

We look beyond simple payments to capture the full spectrum of Stellar's vibrant ecosystem:

- **🧙‍♂️ Soroban Builder Stats:** Contracts deployed and unique user interactions. (Critical for developer reputation!).
- **🤝 dApp Interactions:** Which ecosystem projects did you support the most?
- **🎨 NFT Activity:** New mints collected and top creators supported.
- **💸 Network Volume:** A summary of your general transaction activity.
- **🏆 Your Monthly Persona:** A gamified badge that reflects your unique contribution style.

---

## Ecosystem Impact

This project is designed to support the growth of the Stellar network by:

1.  **Incentivizing Building:** Publicly celebrating developers who ship code creates positive reinforcement. A "Soroban Architect" badge is a social flex that encourages more building.
2.  **Driving Viral Activity:** Every shared Stellar Wrap card is organic marketing for the blockchain, showing the world that Stellar is active and being used.
3.  **Increasing Retention:** Giving users a personalized summary fosters a sense of ownership and encourages them to come back next month to beat their stats.

---

## 🏗️ Architecture

The diagram below shows how on-chain and off-chain components interact in the Stellar Wrap system:

```mermaid
sequenceDiagram
    participant Backend as Backend Service
    participant Admin as Admin Key
    participant User as User Wallet
    participant Contract as Soroban Contract
    participant Frontend as Frontend App

    Note over Backend: 1. Generate wrap data
    Backend->>Backend: Analyze user's on-chain activity
    Backend->>Backend: Compute data_hash (SHA256 of JSON)
    Backend->>Backend: Assign archetype persona

    Note over Backend,Admin: 2. Sign with admin key
    Backend->>Admin: Sign(contract_id + user + period + archetype + data_hash)
    Admin-->>Backend: Ed25519 signature

    Note over Backend,User: 3. Deliver to user
    Backend-->>User: signature + period + archetype + data_hash

    Note over User,Contract: 4. User claims on-chain
    User->>Contract: mint_wrap(user, period, archetype, data_hash, signature)
    Contract->>Contract: Verify user auth (require_auth)
    Contract->>Contract: Verify admin signature (ed25519_verify)
    Contract->>Contract: Check no duplicate (Wrap key)
    Contract->>Contract: Store WrapRecord (persistent)
    Contract->>Contract: Update balance & latest period
    Contract-->>User: Event: (mint, user, period) → archetype

    Note over Frontend,Contract: 5. Frontend reads data
    Frontend->>Contract: get_wrap(user, period)
    Contract-->>Frontend: WrapRecord {timestamp, data_hash, archetype, period}
    Frontend->>Contract: balance_of(user)
    Contract-->>Frontend: wrap count
    Frontend->>Frontend: Display persona & stats
```

---

## 🛠️ Tech Stack

- **Language:** Rust
- **Smart Contract Framework:** Soroban SDK v20.0.0
- **Build Tool:** Cargo
- **Target:** WebAssembly (WASM) for Soroban runtime
- **Testing:** Soroban SDK testutils

---

## 🗺️ Contract Features

- ✅ Admin-controlled initialization
- ✅ Soulbound token (SBT) minting with authorization checks
- ✅ Wrap record storage (timestamp, data hash, archetype)
- ✅ Public query interface for retrieving wrap records
- ✅ Event emission for minting actions
- ✅ Prevention of duplicate wraps per user
- ✅ Contract upgrade mechanism (admin-only WASM upgrade via `upgrade()`)

### Upgrading the Contract

The contract supports in-place WASM upgrades via Soroban's `update_current_contract_wasm`. All persistent storage (wrap records, admin key, etc.) is preserved across upgrades.

**Process:**
1. Upload the new WASM to the Stellar network and note its hash.
2. Call `upgrade(new_wasm_hash)` — requires admin authorization.
3. Soroban validates the hash against the uploaded blob and atomically replaces the code.

Only the admin address can trigger an upgrade. Any call without valid admin authorization will be rejected.

### Storage Schema Migration (v1 → v2)

Schema version is stored in instance storage (`DataKey::SchemaVersion`). `initialize()` sets version `1`. After deploying upgraded WASM that adds the `image_uri` field to `WrapRecord`, the admin must call:

```
migrate(from_version=1, to_version=2)
```

**Procedure:**
1. Upload and deploy new WASM via `upgrade(new_wasm_hash)`.
2. Call `migrate(1, 2)` once — requires admin auth. Emits a `(schema, migrat)` event.
3. Verify `get_schema_version()` returns `2`.
4. Existing v1 records are **lazily migrated** on first `get_wrap` read: upgraded in storage and a `(migrat, user, period)` event is emitted.

`migrate()` is guarded — it only succeeds when the stored version equals `from_version` and `to_version == from_version + 1`. A second call with the same transition panics with `InvalidMigration` (#11).

While schema version is `1`, new mints are stored in v1 format. After migration, new mints use v2 format (`image_uri` included).

### Merkle Batch Claims

For large airdrops, the admin publishes a single merkle root per period instead of signing each mint:

1. **Off-chain:** Build a binary merkle tree over claim leaves (see `scripts/merkle.ts`).
2. **On-chain:** Admin calls `set_merkle_root(period, root)`.
3. **Claim:** Each user calls `claim_wrap(user, period, archetype, data_hash, proof)` with `user.require_auth()`.

**Leaf encoding** (must match contract `compute_merkle_leaf`):

```
leaf = SHA-256( XDR(user) ‖ XDR(period) ‖ XDR(archetype) ‖ XDR(data_hash) )
```

**Internal nodes:** `SHA-256( min(left,right) ‖ max(left,right) )` (32-byte hashes, lexicographic order).

Double-claims are prevented via `MerkleClaimed(user, period)`. Claims produce the same `WrapRecord` and `(mint, user, period)` event as `mint_wrap`.

### User Privacy Opt-Out

Users may hide their wraps from public queries without deleting soulbound records:

- `opt_out(user)` — requires user auth; `get_wrap` / `get_latest_wrap` return `None`
- `opt_in(user)` — restores visibility
- `balance_of` and `verify_data` remain functional for composability
- Admin `revoke_wrap` still works on opted-out users

---

## 📊 Design Decision: On-Chain `WrapCount` and `balance_of`

**Issue [#40](https://github.com/zintarh/stellar-wrap-contract/issues/40) — Considered removing `WrapCount` storage**

### The trade-off

`WrapCount` is a persistent storage entry incremented on every `mint_wrap` call. This means every mint performs two persistent storage writes (the `WrapRecord` and the `WrapCount`). Since mints also emit events, the count *could* be derived off-chain by indexing those events.

### Decision: **Keep `WrapCount` and `balance_of`**

**Rationale:**

1. **On-chain composability.** `balance_of(user)` allows other Soroban contracts to read a user's wrap count in a single storage read. Removing it would make composability with future on-chain logic impossible without an expensive storage scan.
2. **Predictable cost.** One extra persistent write per mint is a fixed, bounded cost. Lazy counting via storage iteration would be unbounded and far more expensive at query time.
3. **Off-chain indexing is unreliable as a source of truth.** Events are not stored in contract state; an indexer can miss events or be unavailable. On-chain state is the canonical source of truth.

**Alternatives considered and rejected:**

| Option | Why rejected |
|---|---|
| Remove `WrapCount`, derive from events | Breaks on-chain composability; indexer dependency |
| Lazy count (iterate storage) | O(n) cost per query; prohibitively expensive at scale |
| Keep as-is | ✅ **Selected** — fixed cost, composable, canonical |

---

## 🔒 Mint Guard Storage Decision

The mint reentrancy guard uses Soroban temporary storage, not persistent storage.

- Temporary storage is cheaper and matches the guard lifecycle (single invocation scope).
- On successful mint, the guard key is removed explicitly.
- On failure paths (panic), the temporary entry is not persisted forever and is naturally cleaned up by Soroban TTL.

## 📝 Contract Interface

### Functions

- `initialize(e: Env, admin: Address)` - Initialize contract with admin (callable once)
- `mint_wrap(e: Env, to: Address, data_hash: BytesN<32>, archetype: Symbol)` - Mint a wrap record (admin only)
- `get_wrap(e: Env, user: Address) -> Option<WrapRecord>` - Retrieve a user's wrap record

### Storage

- `WrapRecord`: Contains `timestamp`, `data_hash`, and `archetype`
- `DataKey::Admin`: Stores the admin address
- `DataKey::Wrap(Address)`: Maps user addresses to their wrap records

### Error Codes

| Code | Error |
| --- | --- |
| 1 | `AlreadyInitialized` |
| 2 | `NotInitialized` |
| 3 | `Unauthorized` |
| 4 | `WrapAlreadyExists` |
| 5 | `InvalidSignature` |
