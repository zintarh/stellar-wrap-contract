//! Shared operational constants for the Stellar Wrap Contract.
//!
//! All time-to-live (TTL) values used for Soroban storage extensions are
//! defined here to ensure a single source of truth across the crate.

/// Persistent-storage TTL representing approximately one calendar year.
///
/// # Ledger-time arithmetic
///
/// Stellar ledgers close every **5 seconds**.  At that cadence:
///
///   - 1 day   = 86 400 s ÷ 5 s/ledger  = 17 280 ledgers
///   - 1 year  ≈ 17 280 × 365           = 6 307 200 ledgers
///
/// # Policy rationale
///
/// A one-year window gives active users ample time between manual TTL
/// renewals while bounding the long-term storage rent footprint.  Keys
/// that are refreshed on every user mutation (wrap count, latest period)
/// effectively never expire for active participants.
pub(crate) const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Maximum number of timelock operations that may be queued at once.
///
/// Guards the `TimelockOps` index vector against unbounded growth (see
/// `timelock::schedule` and `timelock::MAX_PENDING_OPERATIONS`).
pub(crate) const MAX_PENDING_OPERATIONS: u32 = 10;

/// Persistent-storage TTL representing approximately one calendar day.
///
/// Used for short-lived, non-critical data that is migrated from Instance
/// to Persistent storage (e.g. name/symbol metadata).  One day provides
/// a safe buffer for eventual renewal without paying the cost of a full
/// year of rent for entries that may be overwritten soon.
pub(crate) const TTL_TEMP: u32 = 17_280;
