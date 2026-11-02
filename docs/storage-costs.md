# Storage Costs: A Three-Year User

This document estimates what it costs, in XLM, to keep one typical user alive
for three years: 36 monthly wrap records plus the yearly TTL renewals needed to
stop them from expiring. It compares the **v1 layout shipped today** with the
proposed **`UserIndex` layout (v2)**.

The accompanying benchmark lives in `src/storage_cost_test.rs`:

```bash
cargo test storage_cost -- --nocapture
```

It drives both layouts through 36 monthly writes and three yearly renewals and
prints the table below from `Env::cost_estimate().fee().persistent_entry_rent`,
using the Soroban SDK's mainnet fee snapshot. The figures here are the modelled
values for the same sequence, using the snapshot constants directly
(`875` stroops per 1 KB write, rent denominator `1215`, `6_307_200` ledgers per
year), so they can be checked by hand.

## Layouts

**v1 (shipped)** — each month writes:

| Entry | Tier | Approx. size |
| --- | --- | ---: |
| `Wrap(user, period)` | persistent | 100 B |
| `WrapCount(user)` | persistent | 44 B |
| `LatestPeriod(user)` | persistent | 48 B |
| `UserPeriods(user)` (`Vec<u64>`) | persistent | 16 + 8·n B |
| `WrapPeriods(user)` (`Vec<u64>`) | persistent | 16 + 8·n B |
| `LastUpdated(user)` | persistent | 48 B |
| `TotalWrapCount` (shared) | persistent | 44 B |

**v2 (`UserIndex`)** — the four per-user metadata entries collapse into one:

```rust
pub struct UserIndex {
    pub count: u32,
    pub latest_period: u64,
    pub periods: Vec<u64>,
}
```

| Entry | Tier | Approx. size |
| --- | --- | ---: |
| `Wrap(user, period)` | persistent | 100 B |
| `UserIndex(user)` | persistent | 16 + 4 + 8 + 8·n B |
| `LastUpdated(user)` | persistent | 48 B |
| `TotalWrapCount` (shared) | persistent | 44 B |

## Footprint after 36 months

| Layout | Wrap records | User metadata | Total |
| --- | ---: | ---: | ---: |
| v1 | 3,600 B | 792 B | **4,392 B** |
| v2 (`UserIndex`) | 3,600 B | 408 B | **4,008 B** |

`UserIndex` removes **384 B** per three-year user (**8.7%** of the footprint)
and, more importantly, halves the number of metadata entries that must be
renewed every year (six → three).

## Cost table

Steady-state annual rent for the full footprint (the state a three-year user
holds after the last renewal), plus the three-year total assuming that
footprint is held for three years:

| Layout | Annual rent | 3-year rent | Renewal entries / year |
| --- | ---: | ---: | ---: |
| v1 | 1.9482 XLM | 5.8446 XLM | 42 |
| v2 (`UserIndex`) | 1.7779 XLM | 5.3336 XLM | 39 |
| **Saving** | **0.1703 XLM** | **0.5110 XLM** | **3** |

Each renewal also pays a one-off TTL-entry write fee (about 2,500 stroops per
bumped entry, ≈ 0.00025 XLM). v1 performs three more metadata bumps per year
than v2, saving a further **0.00225 XLM** over three years — negligible next to
the rent difference.

These numbers are upper bounds: records minted in month _n_ accrue only
`(36 − n)` months of rent, so the real three-year total is lower for both
layouts. The **relative** saving is stable because both layouts write the same
36 `Wrap` records; the difference comes entirely from the user index.

## Who pays?

TTL renewal is permissionless: `extend_ttl`, `extend_ttl_batch` and
`renew_all_ttls` can be called by anyone, but the transaction fee and the rent
bump are paid by the caller of that transaction. Three options:

1. **User-paid** — the user renews their own records once a year (≈1.9 XLM in
   rent for v1; ≈1.8 XLM for v2).
2. **Sponsored** — a backend or sponsor account renews on the user's behalf.
   `UserIndex` reduces the sponsored amount by ~8.7% and cuts renewal
   bookkeeping by half the metadata entries.
3. **Lapse** — records expire after ~1 year. Not recommended for records the
   product promises to keep.

Because a three-year user's rent is dominated by the 36 `Wrap` records (82% of
the v2 footprint), no index redesign can remove most of the cost; `UserIndex`
is a modest but free improvement that also simplifies the renewal path.

## Limitations

- Entry sizes are the conservative figures from `STORAGE.md` plus the XDR key
  overhead; re-run the benchmark for exact per-entry bytes.
- The rent constants are the SDK's 2026-07 mainnet snapshot. Rent changes with
  network governance; the table should be regenerated when the snapshot is
  refreshed (see `CostEstimate::fee` in `soroban-sdk`).
- The benchmark models both layouts with the probe contract in
  `src/storage_cost_test.rs`; it does not submit transactions to a live network.
