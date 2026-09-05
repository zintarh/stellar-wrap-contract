//! Strongly typed contract events.
//!
//! Defines typed event symbols and data payloads for mint operations,
//! replacing raw `symbol_short!()` strings with a Rust enum. Each
//! variant converts to its corresponding `Symbol` and back, making
//! event names strongly typed throughout the codebase.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol};

use crate::storage_types::{FeeParams, StakeConfig, WrapState};
/// All events emitted by the contract.
///
/// This enum is used for type-safe event publishing via [`publish_event`].
/// Each variant maps to a `(domain, action)` pair and carries the data fields
/// that are published as the event payload.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    // Admin
    AdminInit(Address),
    AdminUpdated(Address, Address),
    AdminPause(bool),
    AdminFeeUpdated(Address, Address, i128),
    AdminFeeCleared,
    AdminUpgrade(u32, BytesN<32>),
    FeeParamsUpdated(FeeParams),

    // Bridge
    BridgeOut(Address, u32, u64, BytesN<32>, u64),
    BridgeRefund(Address, u64, u64),
    BridgeInRej(Address, u32, u64, u64),
    BridgeIn(Address, u32, u64, u64),

    // Burn
    Burn(Address, u64),

    // Governance
    GovPropose(u64, Address, Address),
    GovVote(u64, Address, bool),
    GovExecuted(u64, Address),
    GovDefeated(u64),
    GovCancelled(u64, Address),

    // Merkle
    WhitelistRoot(BytesN<32>),
    WhitelistCleared,

    // Mint
    Mint(Address, u64, Symbol),
    MintTransition(Address, u64, WrapState),
    MintExpire(Address, u64),

    // Revoke
    Revoke(Address, u64, BytesN<32>),

    // Stake
    StakeAdd(Address, i128),
    StakeInit(Address, i128),
    StakeUnstake(Address, i128),
    StakeWithdraw(Address, i128),
    StakeConfig(StakeConfig),

    // Timelock
    TimelockEnabled(u64),
    TimelockSched(BytesN<32>, u64),
    TimelockCancel(BytesN<32>),
    TimelockUpgrade(BytesN<32>),
    TimelockExec(BytesN<32>),

    // Transfer
    TransferBackfill(Address, u32),
    Transfer(Address, Address, u64),
    TransferWithFee(Address, Address, u64, Address, Address, i128),
}

/// Strongly typed event publisher.
#[allow(deprecated)]
pub fn publish_event(e: &Env, event: Event) {
    let v1 = symbol_short!("v1");
    match event.clone() {
        Event::AdminInit(..) => e
            .events()
            .publish((v1, symbol_short!("admin"), symbol_short!("init")), event),
        Event::AdminUpdated(..) => e.events().publish(
            (v1, symbol_short!("admin"), symbol_short!("updated")),
            event,
        ),
        Event::AdminPause(..) => e
            .events()
            .publish((v1, symbol_short!("admin"), symbol_short!("pause")), event),
        Event::AdminFeeUpdated(..) => e
            .events()
            .publish((v1, symbol_short!("admin"), symbol_short!("fee")), event),
        Event::AdminFeeCleared => e.events().publish(
            (v1, symbol_short!("admin"), symbol_short!("fee_clr")),
            event,
        ),
        Event::AdminUpgrade(..) => e.events().publish(
            (v1, symbol_short!("admin"), symbol_short!("upgrade")),
            event,
        ),
        Event::FeeParamsUpdated(..) => e.events().publish(
            (v1, symbol_short!("admin"), symbol_short!("feeparam")),
            event,
        ),

        Event::BridgeOut(..) => e
            .events()
            .publish((v1, symbol_short!("bridge"), symbol_short!("out")), event),
        Event::BridgeRefund(..) => e.events().publish(
            (v1, symbol_short!("bridge"), symbol_short!("refund")),
            event,
        ),
        Event::BridgeInRej(..) => e.events().publish(
            (v1, symbol_short!("bridge"), symbol_short!("in_rej")),
            event,
        ),
        Event::BridgeIn(..) => e
            .events()
            .publish((v1, symbol_short!("bridge"), symbol_short!("in")), event),

        Event::Burn(..) => e
            .events()
            .publish((v1, symbol_short!("wrap"), symbol_short!("burn")), event),

        Event::GovPropose(..) => e
            .events()
            .publish((v1, symbol_short!("gov"), symbol_short!("propose")), event),
        Event::GovVote(..) => e
            .events()
            .publish((v1, symbol_short!("gov"), symbol_short!("vote")), event),
        Event::GovExecuted(..) => e
            .events()
            .publish((v1, symbol_short!("gov"), symbol_short!("executed")), event),
        Event::GovDefeated(..) => e
            .events()
            .publish((v1, symbol_short!("gov"), symbol_short!("defeated")), event),
        Event::GovCancelled(..) => e.events().publish(
            (v1, symbol_short!("gov"), symbol_short!("cancelled")),
            event,
        ),

        Event::WhitelistRoot(..) => e.events().publish(
            (v1, symbol_short!("whitelist"), symbol_short!("root")),
            event,
        ),
        Event::WhitelistCleared => e.events().publish(
            (v1, symbol_short!("whitelist"), symbol_short!("cleared")),
            event,
        ),

        Event::Mint(..) => e
            .events()
            .publish((v1, symbol_short!("wrap"), symbol_short!("mint")), event),
        Event::MintTransition(..) => e
            .events()
            .publish((v1, symbol_short!("wrap"), symbol_short!("trans")), event),
        Event::MintExpire(..) => e
            .events()
            .publish((v1, symbol_short!("wrap"), symbol_short!("expire")), event),

        Event::Revoke(..) => e
            .events()
            .publish((v1, symbol_short!("wrap"), symbol_short!("revoke")), event),

        Event::StakeAdd(..) => e
            .events()
            .publish((v1, symbol_short!("stake"), symbol_short!("add")), event),
        Event::StakeInit(..) => e
            .events()
            .publish((v1, symbol_short!("stake"), symbol_short!("init")), event),
        Event::StakeUnstake(..) => e.events().publish(
            (v1, symbol_short!("stake"), symbol_short!("unstake")),
            event,
        ),
        Event::StakeWithdraw(..) => e.events().publish(
            (v1, symbol_short!("stake"), symbol_short!("withdraw")),
            event,
        ),
        Event::StakeConfig(..) => e
            .events()
            .publish((v1, symbol_short!("stake"), symbol_short!("cfg")), event),

        Event::TimelockEnabled(..) => e.events().publish(
            (v1, symbol_short!("timelock"), symbol_short!("enabled")),
            event,
        ),
        Event::TimelockSched(..) => e.events().publish(
            (v1, symbol_short!("timelock"), symbol_short!("sched")),
            event,
        ),
        Event::TimelockCancel(..) => e.events().publish(
            (v1, symbol_short!("timelock"), symbol_short!("cancel")),
            event,
        ),
        Event::TimelockUpgrade(..) => e.events().publish(
            (v1, symbol_short!("timelock"), symbol_short!("upgrade")),
            event,
        ),
        Event::TimelockExec(..) => e.events().publish(
            (v1, symbol_short!("timelock"), symbol_short!("exec")),
            event,
        ),

        Event::TransferBackfill(..) => e.events().publish(
            (v1, symbol_short!("transfer"), symbol_short!("backfill")),
            event,
        ),
        Event::Transfer(..) => e.events().publish(
            (v1, symbol_short!("transfer"), symbol_short!("transfer")),
            event,
        ),
        Event::TransferWithFee(..) => e
            .events()
            .publish((v1, symbol_short!("transfer"), symbol_short!("fee")), event),
    }
}

/// Strongly typed event data payloads for mint operations.
///
/// Used as the data argument in `e.events().publish()` to provide
/// type-safe event emission instead of raw values.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MintEventData {
    /// A wrap was successfully minted.
    Mint(Address, u64, Symbol),
    /// A wrap's lifecycle state was transitioned.
    Transition(Address, u64, WrapState),
}

/// Strongly typed mint event topic.
///
/// Converted to a `Symbol` for the first topic of mint events via
/// [`MintEventType::to_symbol`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MintEventType {
    /// A single or batch mint of a wrap.
    Mint,
}

impl MintEventType {
    /// Map this variant to its on-chain `Symbol` topic.
    pub fn to_symbol(&self, e: &Env) -> Symbol {
        match self {
            MintEventType::Mint => symbol_short!("mint"),
        }
    }
}
