use soroban_sdk::contracterror;

/// Contract error codes.
///
/// User-facing copy for every variant lives in `error_messages.json` beside
/// this file. `error_messages::every_variant_has_a_mapping` fails when a
/// variant is added here without a matching entry.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    WrapAlreadyExists = 4,
    InvalidSignature = 5,
    InvalidPeriod = 6,
    MigrationAlreadyApplied = 7,
    InvalidStateTransition = 8,
    WrapNotFound = 9,
    NoAdminTransferProposal = 10,
    AdminTransferProposalExists = 11,
    Paused = 12,
    ArithmeticOverflow = 13,
    InvalidFeeParams = 14,
    BatchEmpty = 15,
    BatchTooLarge = 16,
    DuplicateBatchEntry = 17,
    // Staking errors
    StakeTooLow = 18,
    StakeNotFound = 19,
    StakeCooldownActive = 20,
    StakeNotUnstaking = 21,
    StakeCooldownNotElapsed = 22,
    InvalidStakeConfig = 23,
    StakeArithmeticOverflow = 24,
    // Governance proposal errors
    ProposalNotFound = 25,
    ProposalNotActive = 26,
    ProposalAlreadyVoted = 27,
    ProposalVotingPeriodNotEnded = 28,
    ProposalVotingPeriodEnded = 29,
    ProposalDefeated = 30,
    InvalidProposalDuration = 31,
    UserOptedOut = 32,
    // Bridge errors
    BridgeNotInitialized = 33,
    InvalidChain = 34,
    ChainDisabled = 35,
    NonceAlreadyProcessed = 36,
    InvalidBridgePayload = 37,
    // Merkle & Timelock errors
    MerkleRootNotSet = 38,
    InvalidMerkleProof = 39,
    TimelockNotReady = 40,
    TimelockOperationNotFound = 41,
    TimelockOperationExists = 42,
    InvalidTimelockDelay = 43,
    TimelockRequired = 44,
    TimelockAlreadyEnabled = 45,
    // Expiration errors
    WrapNotExpired = 46,
    InvalidExpirationDuration = 47,
    // Transfer errors
    TransferFeeAlreadyConfigured = 48,
    InvalidTransfer = 49,
    TransferInProgress = 50,
    StorageInvariantViolation = 51,
    /// The admin signing key provided to `initialize` is invalid (e.g. all-zero).
    InvalidAdminPubKey = 52,
    InvalidThreshold = 53,
    MerkleProofTooLong = 54,
    // Timelock grace period
    TimelockOperationExpired = 55,
    TimelockOperationNotExpired = 56,
    // Bridge refund state machine errors
    BridgeRequestNotPending = 57,
    BridgeRefundDelayNotElapsed = 58,
    BridgeRequestAlreadyRefunded = 59,
    /// Governance proposal is stale: the admin changed after the proposal was
    /// created, so executing it would revert to a superseded admin (issue #864).
    StaleProposal = 60,
    /// The supplied bridge relayer set is invalid (e.g. it contains duplicate
    /// relayer keys, which would let a single key satisfy multiple slots).
    InvalidRelayerSet = 61,
}

impl ContractError {
    /// Total number of variants in the enum. Used by the coverage test in
    /// `tests/error_variant_coverage.rs` to detect newly added variants that
    /// have not yet been mapped to a producing code path and a failing-path
    /// test. When a variant is added, bump this constant and add the variant
    /// to the `ALL_VARIANTS` table in that test; otherwise CI fails.
    pub const VARIANT_COUNT: u32 = 61;

    /// Every variant in declaration order. The coverage test iterates this
    /// table to assert each variant is reachable and asserted by a test.
    pub const ALL_VARIANTS: [ContractError; Self::VARIANT_COUNT as usize] = [
        ContractError::AlreadyInitialized,
        ContractError::NotInitialized,
        ContractError::Unauthorized,
        ContractError::WrapAlreadyExists,
        ContractError::InvalidSignature,
        ContractError::InvalidPeriod,
        ContractError::MigrationAlreadyApplied,
        ContractError::InvalidStateTransition,
        ContractError::WrapNotFound,
        ContractError::NoAdminTransferProposal,
        ContractError::AdminTransferProposalExists,
        ContractError::Paused,
        ContractError::ArithmeticOverflow,
        ContractError::InvalidFeeParams,
        ContractError::BatchEmpty,
        ContractError::BatchTooLarge,
        ContractError::DuplicateBatchEntry,
        ContractError::StakeTooLow,
        ContractError::StakeNotFound,
        ContractError::StakeCooldownActive,
        ContractError::StakeNotUnstaking,
        ContractError::StakeCooldownNotElapsed,
        ContractError::InvalidStakeConfig,
        ContractError::StakeArithmeticOverflow,
        ContractError::ProposalNotFound,
        ContractError::ProposalNotActive,
        ContractError::ProposalAlreadyVoted,
        ContractError::ProposalVotingPeriodNotEnded,
        ContractError::ProposalVotingPeriodEnded,
        ContractError::ProposalDefeated,
        ContractError::InvalidProposalDuration,
        ContractError::UserOptedOut,
        ContractError::BridgeNotInitialized,
        ContractError::InvalidChain,
        ContractError::ChainDisabled,
        ContractError::NonceAlreadyProcessed,
        ContractError::InvalidBridgePayload,
        ContractError::MerkleRootNotSet,
        ContractError::InvalidMerkleProof,
        ContractError::TimelockNotReady,
        ContractError::TimelockOperationNotFound,
        ContractError::TimelockOperationExists,
        ContractError::InvalidTimelockDelay,
        ContractError::TimelockRequired,
        ContractError::TimelockAlreadyEnabled,
        ContractError::WrapNotExpired,
        ContractError::InvalidExpirationDuration,
        ContractError::TransferFeeAlreadyConfigured,
        ContractError::InvalidTransfer,
        ContractError::TransferInProgress,
        ContractError::StorageInvariantViolation,
        ContractError::InvalidAdminPubKey,
        ContractError::InvalidThreshold,
        ContractError::MerkleProofTooLong,
        ContractError::TimelockOperationExpired,
        ContractError::TimelockOperationNotExpired,
        ContractError::BridgeRequestNotPending,
        ContractError::BridgeRefundDelayNotElapsed,
        ContractError::BridgeRequestAlreadyRefunded,
        ContractError::StaleProposal,
        ContractError::InvalidRelayerSet,
    ];

    /// Numeric code for this variant, matching the `#[repr(u32)]` discriminant.
    pub const fn code(self) -> u32 {
        self as u32
    }
}
