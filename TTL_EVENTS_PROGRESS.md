# TTL Renewal Events Implementation Progress

## Problem Statement
The contract's TTL extension functions (`extend_ttl`, `extend_ttl_batch`, `renew_all_ttls`) change when wrap records will be archived, but **no events are emitted**. This creates two critical problems:

1. **Renewal bots can't track renewals** → May pay twice to renew the same record
2. **Indexers can't warn users** → Users lose access when wraps are unexpectedly archived

## Solution Approach
Emit a `TtlExtended` event with `(user, period, live_until_ledger)` for each renewed record, enabling:
- Renewal bots to avoid double-payment
- Indexers to warn users before archival
- Audit trail of all TTL extensions

## Progress: ~1% Complete

### ✅ What's Been Implemented

**1. Event Definition** (`src/events.rs`)
```rust
pub enum Event {
    // ... existing events ...
    
    /// TTL extended for a wrap record
    /// Data: (user, period, live_until_ledger)
    TtlExtended(Address, u64, u32),
}
```

**2. Event Publisher** (`src/events.rs`)
```rust
Event::TtlExtended(..) => e
    .events()
    .publish((v1, symbol_short!("wrap"), symbol_short!("ttl")), event),
```

Event topics: `("v1", "wrap", "ttl")`  
Event data: `(Address user, u64 period, u32 live_until_ledger)`

**3. Helper Function** (`src/ttl.rs`)
```rust
fn calculate_live_until_ledger(e: &Env) -> u32 {
    e.ledger().sequence() + TTL_ONE_YEAR
}
```

**4. Test Infrastructure** (`src/ttl_events_test.rs`)
- Event structure validation test
- Placeholder tests for `extend_ttl` emission
- Placeholder tests for `extend_ttl_batch` emission
- Documentation of renewal bot use case

**5. Constants Export** (`src/ttl.rs`)
```rust
pub(crate) const TTL_TEMP: u32 = 17_280 * 30;
```

### 🚧 What's NOT Yet Implemented (Remaining 99%)

**1. Event Emission in TTL Functions**
- [ ] `extend_ttl()` - Emit event after successful renewal
- [ ] `extend_ttl_batch()` - Emit event for each renewed period
- [ ] `renew_all_ttls()` - Emit events for metadata renewals (or document why not)

**2. Event Schema Pinning Test** (`src/events_test.rs`)
```rust
EventShape {
    emitter: "extend_ttl, extend_ttl_batch",
    topics: "v1 / wrap / ttl",
    data: "Event::TtlExtended(user, period, live_until_ledger)",
},
```

**3. Integration Tests**
- [ ] Test `extend_ttl` emits correct event with right live_until
- [ ] Test `extend_ttl_batch` emits N events for N periods
- [ ] Test skipped periods (no record) emit no event
- [ ] Test duplicate periods emit duplicate events (idempotent behavior)
- [ ] Test event data is decodable by indexer

**4. Pipeline Integration** (`pipeline/src/decoder.ts`)
```typescript
case 'ttl': {
  const [user, period, liveUntil] = data as [string, bigint, number];
  return {
    type: 'TtlExtended',
    user,
    period,
    liveUntilLedger: liveUntil,
  };
}
```

**5. Database Schema** (`pipeline/src/db.ts`)
```sql
CREATE TABLE ttl_renewals (
  user TEXT NOT NULL,
  period BIGINT NOT NULL,
  live_until_ledger INTEGER NOT NULL,
  renewed_at_ledger INTEGER NOT NULL,
  tx_hash TEXT NOT NULL,
  PRIMARY KEY (user, period, renewed_at_ledger)
);
```

**6. Documentation**
- [ ] Update event catalogue in `README.md`
- [ ] Document renewal bot integration pattern
- [ ] Add example queries for renewal tracking

**7. Cost Analysis**
- [ ] Measure event cost for single renewal
- [ ] Measure event cost for batch (max 100 periods)
- [ ] Document cost tradeoffs in `TTL_EVENTS_DESIGN.md`

## Architecture Notes

### Design Decisions

**Event per record vs batch event:**
- ✅ **Chosen**: One event per renewed record
- **Rationale**: 
  - Easier indexer logic (one event = one renewal)
  - Supports streaming processing
  - Standard pattern (matches mint_wrap_batch)
- **Tradeoff**: Higher event cost for large batches (acceptable)

**live_until_ledger calculation:**
```rust
live_until_ledger = current_ledger + TTL_ONE_YEAR
```
- Simple, predictable calculation
- Matches Soroban's TTL semantics
- Renewal bots can plan ahead

**No event for failed renewals:**
- If record doesn't exist, no event emitted
- Prevents spam from invalid period queries
- Matches current behavior (silent skip)

### Event Schema

```
Topics: ("v1", "wrap", "ttl")
Data: Event::TtlExtended(
  user: Address,           // Address whose wrap was renewed
  period: u64,             // Wrap period that was renewed
  live_until_ledger: u32   // Ledger when record will expire
)
```

### Use Cases

**1. Renewal Bot Tracking**
```typescript
// Bot queries recent TtlExtended events
const renewals = await indexer.getTtlExtensions({ user, since: lastCheck });

// Skip already-renewed records
for (const wrap of wrapsToRenew) {
  const recentRenewal = renewals.find(r => r.period === wrap.period);
  if (recentRenewal && recentRenewal.liveUntilLedger > currentLedger + threshold) {
    console.log(`Wrap ${wrap.period} already renewed, skipping`);
    continue;
  }
  await contract.extend_ttl(user, wrap.period);
}
```

**2. User Archival Warnings**
```typescript
// Indexer checks expiring wraps
const expiringWraps = await db.query(`
  SELECT user, period, live_until_ledger
  FROM ttl_renewals
  WHERE live_until_ledger < $1
`, [currentLedger + WARNING_THRESHOLD]);

// Notify users
for (const wrap of expiringWraps) {
  await notifyUser(wrap.user, `Wrap ${wrap.period} expires in ${daysUntil} days`);
}
```

**3. Renewal Audit Trail**
```sql
-- See all renewals for a user's wrap
SELECT renewed_at_ledger, live_until_ledger, tx_hash
FROM ttl_renewals
WHERE user = ? AND period = ?
ORDER BY renewed_at_ledger DESC;
```

## Implementation Guide

### Step 1: Emit Event in extend_ttl
```rust
pub(crate) fn extend_ttl(e: Env, user: Address, period: u64) {
    let wrap_key = DataKey::Wrap(user.clone(), period);
    
    if !e.storage().persistent().has(&wrap_key) {
        return;
    }
    
    let live_until = calculate_live_until_ledger(&e);
    
    e.storage()
        .persistent()
        .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    
    // NEW: Emit event
    crate::events::publish_event(
        &e,
        crate::events::Event::TtlExtended(user.clone(), period, live_until),
    );
    
    extend_user_metadata_ttl(&e, &user);
    e.storage().instance().extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}
```

### Step 2: Emit Events in extend_ttl_batch
```rust
pub(crate) fn extend_ttl_batch(e: Env, user: Address, periods: Vec<u64>) {
    // ... validation ...
    
    let live_until = calculate_live_until_ledger(&e);
    let mut renewed_any = false;
    
    for period in periods.iter() {
        let wrap_key = DataKey::Wrap(user.clone(), period);
        if e.storage().persistent().has(&wrap_key) {
            e.storage()
                .persistent()
                .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
            
            // NEW: Emit event for each renewed record
            crate::events::publish_event(
                &e,
                crate::events::Event::TtlExtended(user.clone(), period, live_until),
            );
            
            renewed_any = true;
        }
    }
    
    // ... rest of function ...
}
```

### Step 3: Add Event Schema Test
```rust
EventShape {
    emitter: "extend_ttl, extend_ttl_batch",
    topics: "v1 / wrap / ttl",
    data: "Event::TtlExtended(user, period, live_until_ledger)",
},
```

### Step 4: Integration Test
```rust
#[test]
fn test_extend_ttl_emits_event() {
    // ... setup ...
    client.extend_ttl(&user, &period);
    
    let events = env.events().all();
    let ttl_event = events.iter().find(|e| {
        e.0.get(2).unwrap() == symbol_short!("ttl")
    }).unwrap();
    
    // Decode and verify
    let Event::TtlExtended(event_user, event_period, live_until) = ttl_event.1;
    assert_eq!(event_user, user);
    assert_eq!(event_period, period);
    assert!(live_until > env.ledger().sequence());
}
```

## Testing Strategy

### Unit Tests (Done)
- ✅ Event structure compiles
- ✅ Event can be published
- ✅ Placeholder tests document intended behavior

### Integration Tests (TODO)
- Emit event on successful renewal
- Emit N events for batch of N periods
- No event for non-existent period
- Correct live_until_ledger calculation
- Event decodable by pipeline

### Edge Cases (TODO)
- Batch with all invalid periods (no events)
- Batch with mix of valid/invalid (events only for valid)
- Duplicate periods in batch (duplicate events OK)
- renew_all_ttls behavior (metadata-only, no events?)

## Cost Analysis

**Event cost estimate:**
- Single event: ~500 stroops
- Batch of 100: ~50,000 stroops (acceptable tradeoff)

**Bot savings:**
- Avoids redundant renewals
- Saves transaction fees
- Reduces on-chain congestion

**Decision**: Emit per-record events (not batch event) for simplicity.

## Next Steps (Priority Order)

1. **Add event emission in `extend_ttl`** (5 lines)
2. **Add event emission in `extend_ttl_batch`** (5 lines in loop)
3. **Write integration tests** (verify events are emitted)
4. **Add to EVENT_SHAPES catalogue** (events_test.rs)
5. **Update pipeline decoder** (TypeScript)
6. **Add database schema** (SQL migration)
7. **Document renewal bot pattern** (README)

## File Changes Summary

- `src/events.rs` - Added `TtlExtended` event variant and publisher
- `src/ttl.rs` - Added `calculate_live_until_ledger` helper, exported `TTL_TEMP`
- `src/ttl_events_test.rs` - NEW - Event structure and placeholder tests
- `src/lib.rs` - Added test module reference
- `TTL_EVENTS_PROGRESS.md` - NEW - This documentation

## Estimated Completion

- **Done**: ~1% (event definition + infrastructure)
- **Next milestone**: 25% (emit events in both functions)
- **Remaining**: ~75% (tests + pipeline + docs)

## Acceptance Criteria

- ✅ Event defined in Event enum
- ✅ Event publisher added to publish_event
- ✅ Helper function for live_until calculation
- ✅ Test infrastructure created
- ⏳ Event emitted in extend_ttl (TODO)
- ⏳ Event emitted in extend_ttl_batch (TODO)
- ⏳ Integration tests pass (TODO)
- ⏳ EVENT_SHAPES catalogue updated (TODO)
- ⏳ Pipeline decoder handles event (TODO)
- ⏳ Database stores live_until_ledger (TODO)

## Open Questions

1. **Should renew_all_ttls emit events?**
   - It only renews metadata (WrapCount, WrapMetadata), not individual wraps
   - Probably no events needed (admin-only, no user-facing impact)
   - Decision: Document in code comments

2. **Event cost vs bot value tradeoff**
   - Single renewal: negligible cost
   - Batch of 100: ~50k stroops (~$0.005 at current prices)
   - Decision: Per-record events justified by bot utility

3. **Indexer schema: one table or augment existing?**
   - Option A: New `ttl_renewals` table (clean separation)
   - Option B: Add `live_until_ledger` to existing `wraps` table
   - Decision: TBD by pipeline maintainer

---

**Ready for Phase 2**: Add event emission in ttl.rs functions!
