//! Guard and assertion helpers for early validation and defensive checks.
//!
//! Replaces scattered `if cond { panic_with_error!(e, ContractError::X); }` boilerplate
//! with concise, reusable guard functions.

use soroban_sdk::{panic_with_error, Env};

use crate::ContractError;

/// Asserts that a condition is true, panicking with the specified [`ContractError`] if false.
///
/// Marked `#[inline(always)]` to eliminate call-site function overhead and ensure
/// compiled WASM bytecode size is unchanged or smaller compared to manual `if !cond` checks.
#[inline(always)]
pub fn ensure(e: &Env, cond: bool, err: ContractError) {
    if !cond {
        panic_with_error!(e, err);
    }
}

/// Unwraps an `Option<T>`, returning `T` if `Some`, or panicking with [`ContractError`] if `None`.
///
/// Marked `#[inline(always)]` to eliminate call-site function overhead and ensure
/// compiled WASM bytecode size is unchanged or smaller compared to manual `unwrap_or_else`.
#[inline(always)]
pub fn require_some<T>(e: &Env, opt: Option<T>, err: ContractError) -> T {
    match opt {
        Some(val) => val,
        None => panic_with_error!(e, err),
    }
}

/// Unwraps a `Result<T, ContractError>`, returning `T` if `Ok`, or panicking with the error if `Err`.
///
/// Marked `#[inline(always)]` to eliminate call-site function overhead.
#[inline(always)]
pub fn require_ok<T>(e: &Env, res: Result<T, ContractError>) -> T {
    match res {
        Ok(val) => val,
        Err(err) => panic_with_error!(e, err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_ensure_true_does_not_panic() {
        let env = Env::default();
        ensure(&env, true, ContractError::InvalidPeriod);
    }

    #[test]
    #[should_panic]
    fn test_ensure_false_panics() {
        let env = Env::default();
        ensure(&env, false, ContractError::InvalidPeriod);
    }

    #[test]
    fn test_require_some_success() {
        let env = Env::default();
        let value = require_some(&env, Some(42u64), ContractError::WrapNotFound);
        assert_eq!(value, 42);
    }

    #[test]
    #[should_panic]
    fn test_require_some_none_panics() {
        let env = Env::default();
        let _: u64 = require_some(&env, None, ContractError::WrapNotFound);
    }

    #[test]
    fn test_require_ok_success() {
        let env = Env::default();
        let value = require_ok(&env, Ok(99u32));
        assert_eq!(value, 99);
    }

    #[test]
    #[should_panic]
    fn test_require_ok_err_panics() {
        let env = Env::default();
        let _: u32 = require_ok(&env, Err(ContractError::InvalidSignature));
    }
}
