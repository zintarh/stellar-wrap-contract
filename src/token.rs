//! Standard token interface trait for the Stellar Wrap Registry.
//!
//! Extracts standard token interface methods (`name`, `symbol`, `decimals`,
//! `balance_of`) into a dedicated trait, reducing boilerplate in `lib.rs` and
//! promoting interface reuse across Soroban client tooling.
//!
//! **Note on SEP-41 Compatibility:**
//! This contract represents a **soulbound wrap registry** rather than a conventional
//! fungible token. Methods exposed by this interface exist for compatibility, but
//! callers must not treat the contract as a tradable token balance provider.
//!
//! The trait is implemented directly in `lib.rs` with `#[contractimpl]` so
//! the Soroban macro generates client types in the correct crate scope.
//!
//! # Standard Interface Methods
//!
//! | Method       | Description                         |
//! |--------------|-------------------------------------|
//! | `name`       | Registry display name (default: "Stellar Wrap Registry") |
//! | `symbol`     | Registry ticker symbol (default: "WRAP") |
//! | `decimals`   | Number of decimals (always 0; wrap records are indivisible) |
//! | `balance_of` | Count of active wrap records for a user (not a token balance) |

use soroban_sdk::{Address, Env, String};

/// Standard token interface trait for `StellarWrapContract`.
///
/// Implemented in `lib.rs` via `#[contractimpl]` so these methods are
/// automatically exposed as contract functions. The implementations
/// delegate to the `queries` module for storage access.
///
/// This trait exposes token-shaped metadata and lookup functions for a soulbound wrap
/// registry, implementing a deliberate subset of standard token methods.
pub trait TokenInterface {
    /// Returns the registry display name, either the admin-set override or the default.
    fn name(e: Env) -> String;

    /// Returns the registry ticker symbol, either the admin-set override or the default.
    fn symbol(e: Env) -> String;

    /// Returns the number of decimals (`0`).
    ///
    /// `decimals()` intentionally returns `0` because the contract represents discrete,
    /// indivisible wrap records rather than divisible fungible token amounts. There are no
    /// fractional wrap-record units.
    fn decimals(e: Env) -> u32;

    /// Returns the count of active wrap records associated with the given user address.
    ///
    /// **Registry Semantics:**
    /// - The returned value is a **count of wrap records**, NOT a fungible token balance.
    /// - The result must NOT be interpreted as a tradable or spendable token amount.
    /// - The `i128` return type is used for token interface compatibility; under the hood,
    ///   it is a widened `u32` wrap record counter (`WrapCount`) stored in persistent storage.
    fn balance_of(e: Env, user: Address) -> i128;
}
