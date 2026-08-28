# Security Fix: mint_wrap_batch Bypasses User Opt-Out Check

## Issue Summary
The `mint_wrap_batch` function in `src/mint.rs` completely bypassed the user opt-out check that was properly implemented in the single `mint_wrap` function. This allowed users to circumvent their opt-out status by routing mint requests through the batch endpoint instead.

## Root Cause
The `mint_wrap` function correctly checked for opted-out users:
```rust
if e.storage().persistent().has(&DataKey::OptOut(user.clone())) {
    panic_with_error!(e, ContractError::UserOptedOut);
}
```

But `mint_wrap_batch` had no equivalent check in either its aggregated signature path or individual signature path, making the opt-out guarantee ineffective.

## Security Impact
- **Opt-out bypass**: Users could not reliably prevent future wraps from being minted
- **Contract guarantee violation**: The advertised "user-controlled guarantee" was not enforced consistently
- **Privacy/consent concern**: Users who opted out could still have wraps created without their consent

## Solution Implemented

### 1. Extracted Shared Helper Function
Created a reusable opt-out validation function:
```rust
/// Validates that a user has not opted out of wrap minting.
/// 
/// # Panics
/// - [`ContractError::UserOptedOut`] if the user has opted out.
fn require_user_not_opted_out(e: &Env, user: &Address) {
    if e.storage().persistent().has(&DataKey::OptOut(user.clone())) {
        panic_with_error!(e, ContractError::UserOptedOut);
    }
}
```

### 2. Updated mint_wrap to Use Helper
```rust
// Before
if e.storage().persistent().has(&DataKey::OptOut(user.clone())) {
    panic_with_error!(e, ContractError::UserOptedOut);
}

// After
require_user_not_opted_out(&e, &user);
```

### 3. Added Opt-Out Checks to Batch Validation

**Aggregated Signature Path:**
```rust
if let Some(agg_sig) = aggregated_signature {
    for item in items.iter() {
        validate_period(&e, item.period);
        validate_payload_version(&e, item.payload_version);
        require_user_not_opted_out(&e, &item.user); // NEW
        item.user.require_auth();
    }
    // ... signature verification
}
```

**Individual Signature Path:**
```rust
} else {
    for item in items.iter() {
        validate_period(&e, item.period);
        validate_payload_version(&e, item.payload_version);
        require_user_not_opted_out(&e, &item.user); // NEW
        item.user.require_auth();
        // ... signature verification
    }
}
```

## Acceptance Criteria Status

✅ **mint_wrap_batch panics with ContractError::UserOptedOut**: Implemented for both paths  
✅ **Check runs in validation loop**: Added before any state changes  
✅ **No partial state written**: Validation happens before processing loop  
✅ **Same check applied to aggregated-signature path**: Both paths protected  

## Test Coverage Added

### 1. Basic Opt-Out Enforcement
- `test_mint_wrap_rejects_opted_out_user`: Verifies single mint rejection
- `test_mint_wrap_batch_rejects_opted_out_user_aggregated_signature`: Tests aggregated path
- `test_mint_wrap_batch_rejects_opted_out_user_individual_signatures`: Tests individual path

### 2. Atomicity Verification
- `test_mint_wrap_batch_partial_failure_no_state_written`: Ensures no partial state when batch fails

### 3. Opt-Out/Opt-In Workflow
- `test_opt_out_opt_in_cycle`: Verifies complete opt-out and opt-in functionality

## Security Properties Restored

| Property | Before | After |
|----------|--------|-------|
| **Opt-out guarantee** | Bypassable via batch | Consistently enforced |
| **Validation timing** | N/A for batch | Early validation phase |
| **Atomicity** | Undefined on failure | No partial state on opt-out failure |
| **API consistency** | Inconsistent between endpoints | Consistent behavior |

## Implementation Details

### Validation Order
The opt-out check is strategically placed in the validation loop to ensure:
1. **Early failure**: Detected before any expensive operations
2. **Atomicity**: No state changes if any item fails validation
3. **Clear errors**: Returns specific `UserOptedOut` error code (#32)

### Error Handling
- Error code: `ContractError::UserOptedOut` (#32)
- Consistent across single and batch operations
- Fails entire batch if any user is opted out
- No partial processing or state corruption

### Performance Impact
- Minimal: Simple storage key existence check per item
- Early validation prevents wasted processing on invalid batches
- No additional storage or computation overhead

The fix ensures that user opt-out preferences are consistently respected across all mint operations, restoring the security guarantee that opted-out users cannot have new wraps minted for them through any pathway.