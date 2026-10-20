#![cfg(test)]

use crate::storage_types::PauseFlags;

#[test]
fn test_pause_flags_none() {
    let flags = PauseFlags::none();
    assert!(!flags.mint);
    assert!(!flags.transfer);
    assert!(!flags.revoke_burn);
    assert!(!flags.bridge);
    assert!(!flags.ttl);
    assert!(!flags.stake);
    assert!(!flags.is_any_paused());
}

#[test]
fn test_pause_flags_all() {
    let flags = PauseFlags::all();
    assert!(flags.mint);
    assert!(flags.transfer);
    assert!(flags.revoke_burn);
    assert!(flags.bridge);
    assert!(flags.ttl);
    assert!(flags.stake);
    assert!(flags.is_any_paused());
}

#[test]
fn test_pause_flags_partial() {
    let flags = PauseFlags {
        mint: true,
        transfer: false,
        revoke_burn: false,
        bridge: true,
        ttl: false,
        stake: false,
    };
    assert!(flags.mint);
    assert!(!flags.transfer);
    assert!(flags.bridge);
    assert!(flags.is_any_paused());
}

#[test]
fn test_pause_flags_is_any_paused() {
    // All off
    let mut flags = PauseFlags::none();
    assert!(!flags.is_any_paused());

    // One flag on
    flags.mint = true;
    assert!(flags.is_any_paused());

    // All but one on
    flags = PauseFlags::all();
    flags.stake = false;
    assert!(flags.is_any_paused());
}
