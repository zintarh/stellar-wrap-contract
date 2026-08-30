//! Strongly typed contract events.
//!
//! Enforces a consistent topic convention for all events:
//! `(version, domain, action, ..keys)`
//!
//! - `version`: `v1` (Symbol)
//! - `domain`: e.g. `admin`, `bridge`, `wrap`, `gov`, `whitelist`, `stake`, `timelock`, `transfer` (Symbol, <= 9 chars)
//! - `action`: e.g. `init`, `updated`, `pause`, `fee` (Symbol, <= 9 chars)
//! - `keys`: optional extra keys if it fits in 4 topic limit. But here we just use `(version, domain, action)` or similar, and place data in the payload.
//!
//! Replace inline `e.events().publish()` calls with typed enum values, reducing the risk of typos and improving discoverability.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol};

use crate::storage_types::{StakeConfig, WrapState};

/// All events emitted by the contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    // Admin
    AdminInit { admin: Address },
    AdminUpdated { old_admin: Address, new_admin: Address },
    AdminPause { paused: bool },
    AdminFeeUpdated { token: Address, recipient: Address, amount: i128 },
    AdminFeeCleared,
    AdminUpgrade { version: u32, wasm_hash: BytesN<32> },

    // Bridge
    BridgeOut { user: Address, destination_chain: u32, nonce: u64, recipient_address: BytesN<32>, period: u64 },
    BridgeRefund { user: Address, period: u64, nonce: u64 },
    BridgeInRej { recipient: Address, source_chain: u32, nonce: u64, period: u64 },
    BridgeIn { recipient: Address, source_chain: u32, nonce: u64, period: u64 },

    // Burn
    Burn { user: Address, period: u64 },

    // Governance
    GovPropose { id: u64, proposer: Address, proposed_admin: Address },
    GovVote { id: u64, voter: Address, support: bool },
    GovExecuted { id: u64, new_admin: Address },
    GovDefeated { id: u64 },
    GovCancelled { id: u64, caller: Address },

    // Merkle
    WhitelistRoot { root: BytesN<32> },
    WhitelistCleared,

    // Mint
    Mint { user: Address, period: u64, archetype: Symbol },
    MintTransition { user: Address, period: u64, state: WrapState },
    MintExpire { user: Address, period: u64 },

    // Revoke
    Revoke { user: Address, period: u64, reason_hash: BytesN<32> },

    // Stake
    StakeAdd { user: Address, amount: i128 },
    StakeInit { user: Address, amount: i128 },
    StakeUnstake { user: Address, amount: i128 },
    StakeWithdraw { user: Address, withdrawn: i128 },
    StakeConfig { config: StakeConfig },

    // Timelock
    TimelockEnabled { delay: u64 },
    TimelockSched { id: BytesN<32>, eta: u64 },
    TimelockCancel { id: BytesN<32> },
    TimelockUpgrade { wasm_hash: BytesN<32> },
    TimelockExec { id: BytesN<32> },

    // Transfer
    TransferBackfill { user: Address, count: u32 },
    Transfer { from: Address, to: Address, period: u64, fee: Option<(Address, Address, i128)> },
}

/// Strongly typed event publisher.
pub fn publish_event(e: &Env, event: Event) {
    let v1 = symbol_short!("v1");
    match event.clone() {
        Event::AdminInit { .. } => e.events().publish((v1, symbol_short!("admin"), symbol_short!("init")), event),
        Event::AdminUpdated { .. } => e.events().publish((v1, symbol_short!("admin"), symbol_short!("updated")), event),
        Event::AdminPause { .. } => e.events().publish((v1, symbol_short!("admin"), symbol_short!("pause")), event),
        Event::AdminFeeUpdated { .. } => e.events().publish((v1, symbol_short!("admin"), symbol_short!("fee")), event),
        Event::AdminFeeCleared => e.events().publish((v1, symbol_short!("admin"), symbol_short!("fee_clr")), event),
        Event::AdminUpgrade { .. } => e.events().publish((v1, symbol_short!("admin"), symbol_short!("upgrade")), event),
        
        Event::BridgeOut { .. } => e.events().publish((v1, symbol_short!("bridge"), symbol_short!("out")), event),
        Event::BridgeRefund { .. } => e.events().publish((v1, symbol_short!("bridge"), symbol_short!("refund")), event),
        Event::BridgeInRej { .. } => e.events().publish((v1, symbol_short!("bridge"), symbol_short!("in_rej")), event),
        Event::BridgeIn { .. } => e.events().publish((v1, symbol_short!("bridge"), symbol_short!("in")), event),

        Event::Burn { .. } => e.events().publish((v1, symbol_short!("wrap"), symbol_short!("burn")), event),
        
        Event::GovPropose { .. } => e.events().publish((v1, symbol_short!("gov"), symbol_short!("propose")), event),
        Event::GovVote { .. } => e.events().publish((v1, symbol_short!("gov"), symbol_short!("vote")), event),
        Event::GovExecuted { .. } => e.events().publish((v1, symbol_short!("gov"), symbol_short!("executed")), event),
        Event::GovDefeated { .. } => e.events().publish((v1, symbol_short!("gov"), symbol_short!("defeated")), event),
        Event::GovCancelled { .. } => e.events().publish((v1, symbol_short!("gov"), symbol_short!("cancelled")), event),

        Event::WhitelistRoot { .. } => e.events().publish((v1, symbol_short!("whitelist"), symbol_short!("root")), event),
        Event::WhitelistCleared => e.events().publish((v1, symbol_short!("whitelist"), symbol_short!("cleared")), event),

        Event::Mint { .. } => e.events().publish((v1, symbol_short!("wrap"), symbol_short!("mint")), event),
        Event::MintTransition { .. } => e.events().publish((v1, symbol_short!("wrap"), symbol_short!("trans")), event),
        Event::MintExpire { .. } => e.events().publish((v1, symbol_short!("wrap"), symbol_short!("expire")), event),
        
        Event::Revoke { .. } => e.events().publish((v1, symbol_short!("wrap"), symbol_short!("revoke")), event),

        Event::StakeAdd { .. } => e.events().publish((v1, symbol_short!("stake"), symbol_short!("add")), event),
        Event::StakeInit { .. } => e.events().publish((v1, symbol_short!("stake"), symbol_short!("init")), event),
        Event::StakeUnstake { .. } => e.events().publish((v1, symbol_short!("stake"), symbol_short!("unstake")), event),
        Event::StakeWithdraw { .. } => e.events().publish((v1, symbol_short!("stake"), symbol_short!("withdraw")), event),
        Event::StakeConfig { .. } => e.events().publish((v1, symbol_short!("stake"), symbol_short!("cfg")), event),

        Event::TimelockEnabled { .. } => e.events().publish((v1, symbol_short!("timelock"), symbol_short!("enabled")), event),
        Event::TimelockSched { .. } => e.events().publish((v1, symbol_short!("timelock"), symbol_short!("sched")), event),
        Event::TimelockCancel { .. } => e.events().publish((v1, symbol_short!("timelock"), symbol_short!("cancel")), event),
        Event::TimelockUpgrade { .. } => e.events().publish((v1, symbol_short!("timelock"), symbol_short!("upgrade")), event),
        Event::TimelockExec { .. } => e.events().publish((v1, symbol_short!("timelock"), symbol_short!("exec")), event),

        Event::TransferBackfill { .. } => e.events().publish((v1, symbol_short!("transfer"), symbol_short!("backfill")), event),
        Event::Transfer { .. } => e.events().publish((v1, symbol_short!("transfer"), symbol_short!("transfer")), event),
    }
}
