use soroban_sdk::contracterror;

/// Errors returned by the staking contract.
///
/// Amounts are accepted as `i128` to stay compatible with the SEP-41 token
/// interface (`transfer`/`transfer_from` take `i128`). Because `i128` is
/// signed, every entry point that accepts an amount must explicitly reject
/// non-positive values instead of relying on a panic or a silent no-op.

///
/// Amounts are accepted as `i128` to stay compatible with the SEP-41 token
/// interface (`transfer`/`transfer_from` take `i128`). Because `i128` is
/// signed, every entry point that accepts an amount must explicitly reject
/// non-positive values instead of relying on a panic or a silent no-op.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// The contract has not been initialized yet.
    NotInitialized = 1,
    /// The contract has already been initialized.
    AlreadyInitialized = 2,
    /// The caller is not authorized to perform this action.
    Unauthorized = 3,
    /// The requested stake amount is zero or negative.
    ///
    /// `stake` only accepts strictly positive amounts; zero and negative
    /// values are rejected with this dedicated error rather than panicking
    /// or silently succeeding.
    InvalidAmount = 4,
    /// An arithmetic operation overflowed.
    ///
    /// The release profile disables overflow checks, so `stake`, `unstake`
    /// and the `total_staked` bookkeeping use checked arithmetic and surface
    /// this error instead of wrapping (see #651).
    ArithmeticOverflow = 5,
    /// The user does not have enough staked balance for the operation.
    InsufficientStake = 6,
    /// The contract is paused and cannot process this operation.
    ContractPaused = 7,
}

/// Backwards-compatible alias for the previous error type name.
///
/// Existing call sites that reference `Error` continue to compile while the
/// canonical name is now `ContractError`.
pub use ContractError as Error;
