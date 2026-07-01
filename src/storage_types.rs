
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    AdminPubKey,
    SchemaVersion,
    Wrap(Address, u64),
    WrapCount(Address),
    LatestPeriod(Address),
    MintGuard(Address),
    AllowedArchetypes,
    MerkleRoot(u64),
    MerkleClaimed(Address, u64),
    UserOptOut(Address),
    /// Ordered list of all periods minted for a user (Vec<u64>)
    WrapPeriods(Address),
}

