# Wallet and Explorer Integration Guide

This contract stores discrete wrap records for Stellar addresses. Integrations should display each record as a collectible credential or membership card. A wrap is not a fungible token balance: `balance_of(user)` is a count only, and a wallet must not show amount, allowance, or standard token-transfer controls.

## 1. Identify the contract

Treat the contract ID as the primary identity. Load it from a trusted deployment registry or an application configuration that you control, and verify that it matches the expected network. Do not trust a contract because its `name`, `symbol`, archetype, description, or image looks familiar; those values are display metadata, not proof of issuer identity. Check the deployed interface/version against the version your client supports before relying on newer query methods.

## 2. List a wallet's wraps

Call `get_wraps(user, start, limit)` and paginate with a `u32` offset and limit. `get_all_wraps_for_user(user)` is convenient for a small collection, but its 200-record guidance is not yet enforced; use pagination for larger wallets. The returned vector contains `WrapRecord` values. A representative SDK-decoded result is:

```json
[
  {
    "created_at": 1761000000,
    "data_hash": "9f86d081884c7d659a2feaa0c55ad015...",
    "archetype": "builder",
    "period": 202610,
    "lifecycle": { "state": "Active", "updated_at": 1761000000 },
    "description": "October builder cohort",
    "image_url": "https://images.example.org/wraps/builder-202610.png"
  }
]
```

The exact transport representation depends on the Soroban SDK: RPC contract values are XDR-encoded and should be decoded with the generated client rather than parsed as this JSON directly. Optional metadata fields can be absent. Do not infer a stable sort order from the vector; retain the returned order or sort explicitly in the UI.

For an expiration-aware list, use `get_wraps_v2(user, start, limit)`. Each result wraps the same record with `is_expired` and `live_until_ledger`; show expiration separately from the stored lifecycle state. The `period` is a `YYYYMM` identifier, not a timestamp.

## 3. Render a wrap card

A useful card can show:

- Image, with a neutral fallback when `image_url` is absent or fails to load.
- `description`, falling back to the `archetype` label.
- Period (`YYYY-MM`), wallet address, and lifecycle state.
- An expired indicator from the v2 query when available.
- A shortened `data_hash` and a verification affordance.

`description` and `image_url` are optional owner-set metadata. Treat both as untrusted input: escape text, allow only safe HTTPS image URLs (or proxy them through your image service), and never use the URL as HTML. A simple layout is an image at the top, archetype and period beneath it, a lifecycle badge beside the title, then the owner and verification details in secondary text.

## 4. Verify displayed data

`data_hash` commits to off-chain bytes; it does not contain the original payload or prove what the payload means. Retrieve the payload from your own trusted source, compute its SHA-256 digest, and compare it with the 32-byte hash. You can also call `verify_data(user, period, data)` with the exact raw bytes; it returns `true` only when the wrap exists and the bytes match. Display “verified” only after this check succeeds. If the payload is unavailable, show the hash as unverified rather than guessing.

## 5. Handle state and actions safely

The current lifecycle states are `Active`, `Bridged`, and `Expired`. Re-read the contract before showing an action that depends on current state. A wrap is a discrete record, so do not map its count to a currency amount or offer standard SEP-41 transfer/approval UI. The contract has a separate `transfer_wrap(from, to, period)` operation with record-specific authorization and fee semantics; expose it only as an explicit wrap transfer after confirming the user understands what moves.

`set_wrap_metadata(user, period, description, image_url)` is owner-authorized. It changes display metadata, not the committed payload or issuer identity.
