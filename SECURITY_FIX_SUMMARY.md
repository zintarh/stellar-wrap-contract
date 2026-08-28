# Security Fix: verify_ed25519 Panic on Large Payloads

## Issue Summary
The `verify_ed25519` function in `src/signature.rs` had a critical security vulnerability that caused VM traps when processing payloads larger than 512 bytes. This affected batch mint operations with more than ~5 items, permanently disabling the aggregated-signature path for larger batches.

## Root Cause
```rust
let mut msg = [0u8; 512];
let len = message.len() as usize;
message.copy_into_slice(&mut msg[..len]); // PANIC: index out of bounds when len > 512
```

The function used a fixed 512-byte stack buffer but didn't validate payload size before copying, causing raw Rust panics instead of returning `ContractError::InvalidSignature`.

## Solution Implemented

### 1. Fixed Buffer Management
- **Before**: Fixed 512-byte stack array that panicked on overflow
- **After**: Heap-allocated `Vec<u8>` sized to actual message length
- **Benefit**: Eliminates buffer overrun possibility while supporting larger payloads

### 2. Added Payload Size Validation
- **New Constant**: `MAX_SIGNATURE_PAYLOAD_SIZE = 16384` (16KB)
- **Validation**: Explicit size check before processing
- **Error Handling**: Returns `ContractError::InvalidSignature` for oversized payloads

### 3. Added Batch Size Constants
- **New Constant**: `MAX_BATCH_PAYLOAD_SIZE = 16384` in `mint.rs`
- **Purpose**: Documents expected maximum batch payload size
- **Alignment**: Matches `MAX_SIGNATURE_PAYLOAD_SIZE` for consistency

## Code Changes

### src/signature.rs
```rust
// NEW: Added payload size limit constant
pub const MAX_SIGNATURE_PAYLOAD_SIZE: usize = 16384; // 16KB

fn verify_ed25519(
    public_key: &BytesN<32>,
    message: &Bytes,
    signature: &BytesN<64>,
) -> Result<(), ContractError> {
    let verifying_key = VerifyingKey::from_bytes(&public_key.to_array())
        .map_err(|_| ContractError::InvalidSignature)?;
    let sig = Signature::from_bytes(&signature.to_array());

    let len = message.len() as usize;
    
    // NEW: Validate payload size before processing
    if len > MAX_SIGNATURE_PAYLOAD_SIZE {
        return Err(ContractError::InvalidSignature);
    }

    // NEW: Heap-allocated buffer sized to message length
    extern crate alloc;
    use alloc::vec::Vec;
    
    let mut msg_bytes = Vec::with_capacity(len);
    msg_bytes.resize(len, 0u8);
    message.copy_into_slice(&mut msg_bytes);

    verifying_key
        .verify_strict(&msg_bytes, &sig)
        .map_err(|_| ContractError::InvalidSignature)
}
```

### src/mint.rs
```rust
pub const MAX_BATCH_SIZE: u32 = 100;

// NEW: Added batch payload size documentation
pub const MAX_BATCH_PAYLOAD_SIZE: usize = 16384;
```

## Acceptance Criteria Status

✅ **verify_ed25519 never indexes past buffer end**: Fixed with heap allocation and size validation  
✅ **Over-long payloads return ContractError::InvalidSignature**: Added explicit size check  
✅ **mint_wrap_batch succeeds with MAX_BATCH_SIZE items**: Increased limit to 16KB supports 100 items  
✅ **Regression tests added**: Comprehensive test suite covering edge cases  

## Test Coverage Added

1. **`test_verify_ed25519_rejects_oversized_payload`**: Verifies size limit enforcement
2. **`test_batch_mint_with_max_size`**: Tests full 100-item batch with aggregated signature
3. **`test_single_mint_with_maximum_archetype_symbol`**: Tests worst-case single mint
4. **`test_batch_payload_size_within_limits`**: Validates payload size calculations

## Security Impact

- **Before**: VM trap on large payloads → opaque host error
- **After**: Controlled error handling → proper `ContractError::InvalidSignature`
- **Batch Processing**: Now supports full MAX_BATCH_SIZE (100 items) reliably
- **Resource Usage**: Bounded memory allocation prevents DoS via oversized payloads

## Performance Impact

- **Memory**: Changed from fixed 512-byte stack allocation to variable heap allocation
- **CPU**: Minimal overhead from size validation check
- **Scalability**: Supports payloads up to 16KB (32x increase from 512 bytes)

The fix ensures robust batch processing while maintaining security boundaries and proper error handling.