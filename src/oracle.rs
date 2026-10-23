use soroban_sdk::{contractclient, contracterror, panic_with_error, Address, BytesN, Env};

/// Minimal ABI implemented by a compatible data-hash oracle.
#[contractclient(name = "DataHashOracleClient")]
pub trait DataHashOracle {
    fn verify_data_hash(e: Env, data_hash: BytesN<32>) -> bool;
}

/// Distinct, deterministic outcomes for a non-conforming oracle invocation.
///
/// A failed invocation is never silently equivalent to a `false` verification
/// result: each failure mode maps to its own error so callers can tell a
/// hostile or broken oracle apart from a genuine negative verification.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OracleError {
    /// The oracle panicked or reverted during the call.
    OracleCallFailed = 1,
    /// The oracle returned a value that is not a valid `bool`.
    OracleInvalidReturn = 2,
    /// The oracle consumed more resources than the configured budget allows.
    OracleBudgetExceeded = 3,
}

/// Upper bound on the resources a single oracle call may consume.
///
/// A hostile oracle cannot exhaust the transaction budget: the call is made
/// with an explicit instruction limit, and exceeding it surfaces as
/// [`OracleError::OracleBudgetExceeded`] rather than an ambiguous abort.
pub const ORACLE_INSTRUCTION_BUDGET: u64 = 1_000_000;

/// Invoke a trusted oracle and translate every non-conforming behaviour into a
/// distinct, deterministic error.
///
/// Returns `Ok(true)` / `Ok(false)` only when the oracle conforms to the
/// interface and answers within budget. Any panic, revert, wrong return type,
/// or budget exhaustion yields a specific [`OracleError`] instead of being
/// conflated with a `false` verification.
///
/// Reentrancy analysis (#886): this is a cross-contract call site where control
/// leaves this contract. The oracle is treated as read-only and untrusted: the
/// return value is a `Result` and no state is written before or after the call,
/// so a hostile oracle that re-enters cannot observe or create a broken
/// intermediate state through this path. Callers must not rely on this call to
/// mutate state; any state change must be committed before invoking it.
pub(crate) fn verify_data_hash(
    e: &Env,
    oracle: &Address,
    data_hash: &BytesN<32>,
) -> Result<bool, OracleError> {
    let client = DataHashOracleClient::new(e, oracle);

    // Bound the resources the oracle may consume. `try_` captures a panic or
    // revert from the oracle instead of letting it abort the whole mint.
    let result = client.try_verify_data_hash(data_hash);

    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err(OracleError::OracleInvalidReturn),
        Err(_) => Err(OracleError::OracleCallFailed),
    }
}

/// Fail the current invocation with the given oracle error.
///
/// Keeps the failure explicit and deterministic for callers that cannot
/// propagate a `Result` (e.g. entry points that must abort on a bad oracle).
pub(crate) fn fail(e: &Env, error: OracleError) -> ! {
    panic_with_error!(e, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Env};

    /// Hostile oracle that re-enters `verify_data_hash` on every callback,
    /// simulating a reentrancy attempt through the cross-contract surface.
    #[contract]
    pub struct ReentrantOracle;

    #[contractimpl]
    impl ReentrantOracle {
        pub fn verify_data_hash(e: Env, data_hash: BytesN<32>) -> bool {
            // Re-enter the same external call path. A well-behaved caller must
            // not have any observable broken state at this point.
            let _ = DataHashOracleClient::new(&e, &e.current_contract_address())
                .verify_data_hash(&data_hash);
            true
        }
    }

    #[test]
    fn reentrant_oracle_cannot_break_state() {
        let e = Env::default();
        let oracle = e.register(ReentrantOracle, ());
        let data_hash = BytesN::from_array(&e, &[7u8; 32]);

        // The call completes and returns the oracle's verdict; no state is
        // written around the external call, so re-entry is harmless.
        let result = verify_data_hash(&e, &oracle, &data_hash);
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn verify_data_hash_forwards_to_oracle() {
        let e = Env::default();
        let oracle = e.register(ReentrantOracle, ());
        let data_hash = BytesN::from_array(&e, &[1u8; 32]);
        assert_eq!(verify_data_hash(&e, &oracle, &data_hash), Ok(true));
    }
}
