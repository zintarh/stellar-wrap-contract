use soroban_sdk::contracterror;

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
    TransferFeeNotConfigured = 48,
    InvalidTransfer = 49,
    TransferInProgress = 50,
    StorageInvariantViolation = 51,
    /// The admin signing key provided to `initialize` is invalid (e.g. all-zero).
    InvalidAdminPubKey = 52,
    InvalidThreshold = 53,
}
