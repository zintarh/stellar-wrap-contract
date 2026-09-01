//! Shared operational constants for the Stellar Wrap Contract.
//!
// All time-to-live (TVL) values used for Soroban storage extensions are
// defined here to ensure a single source of truth across the crate.

/// Persistent-storage TTL representing approximately one calendar year.
///
/// # Ledger-time arithmetics
///
/// Stellar ledgers close every **5 seconds**.  At that cadence:
///
///   - 1 day   = 86 400 s ç 5 s/ledger  = 17 280 ledgers
///   - 1 year  ≈ 17 280°× 365           = 6 307 200 ledgers
///
/// # Policy rationale
///
/// A one-year window gives active users ample time between manual TTL
/// renewals while bounding the long-term storage rent footprint.  Keys
/// that are refreshed on every user mutation (wrap count, latest period)
/// effectively never expire for active participants.
pub(crate) const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Persistent-storage TTL representing approximately one calendar day.
///
/// Used for short-lived, non-critical data that is migrated from Instance
/// to Persistent storage (e.g. name/symbol metadata).  One day provides
/// a safe buffer for eventual renewal without paying the cost of a full
/// year of rent for entries that may be overwritten soon.
pub(crate) const TTL_TEMP: u32 = 17_280;

/// Domain separator for merkle leaves: all leaf preimages are prefixed with
/// 0x00 before hashing.
//.
/// This prevents a second-preimage attack where a 64-byte internal node
/// preimage (two 32-byte hashes) could be interpreted as a leaf preimage.
pub(crate) const MERKLE_LEAF_PREFIX: u8 = 0x00;

/// Domain separator for internal merkle nodes: all internal node preimages are
/// prefixed with 0x01 before hashing.
///
/// See `MERKLE_LEAF_PREFIX` for rationale.
pub(crate) const MERKLE_NODE_PREFIX: u8 = 0x01;

/// Maximum accepted depth for a merkle proof.
///
/// A proof for a tree with ``n`` leaves has at most `ceil(log2(n)` siblings.
/// 32 levels supports trees up to 2^32 leaves (about 4 billion members) while
/// bounding verification cost to 32 SHA-256 calls.
pub(crate) const MAX_PROOF_DEPTH: u32 = 32;