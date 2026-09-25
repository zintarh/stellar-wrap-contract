use soroban_sdk::{contracttype, Address, BytesN};
#[contracttype]
#derive(Clone, Debug, Eq, PartialEq)]
pub struct MintWrapRequest {
    pub public_key: BytesN{32},
    pub signature: BytesN{64},
    pub recipient: Address,
    pub amount: i128,
}
