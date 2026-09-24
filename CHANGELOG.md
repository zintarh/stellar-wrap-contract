# Changelog

## 0.1.0

### Contract interface snapshot

This release documents the current public Soroban interface for the Stellar Wrap contract and records the client-facing contract changes that should be considered by backend and frontend integrations.

#### Current write methods

- `initialize(e, admin, admin_pubkey)`
- `update_admin(e, new_admin)`
- `pause(e)` / `unpause(e)`
- `migrate(e, version)`
- `mint_wrap(e, user, period, archetype, data_hash, payload_version, signature)`

#### Current read methods

- `get_wrap(e, user, period)`
- `get_mint_timestamp(e, user, period)`
- `balance_of(e, user)`
- `total_wrap_count(e)`
- `verify_data(e, user, period, data)`
- `get_latest_wrap(e, user)`
- `get_wraps(e, user, start, limit)`
- `get_admin(e)`
- `health(e)`
- `name(e)`, `symbol(e)`, `decimals(e)`
- `migration_version(e)`
- `storage_schema_version(e)`

### Breaking changes and migration notes

- Mint signatures are now versioned. Clients must sign the canonical payload with the current payload-versioning scheme and pass the `payload_version` argument to `mint_wrap`. Legacy integrations that only prepared signatures for the older layout should update their signer flow before deployment.
- New deployments persist `CURRENT_STORAGE_SCHEMA_VERSION` during `initialize`, and clients can read it with `storage_schema_version`. Upgrades that preserve storage encoding must leave this value unchanged. Upgrades that change the schema must add compatible keys or explicitly migrate existing values, use the monotonic `migrate` operation to record the migration, and publish the new schema version only after the migration succeeds. Deployments created before schema tracking return `0` until deliberately migrated.
- The documented public interface now includes additional query helpers such as `get_mint_timestamp`, `total_wrap_count`, and `get_wraps`. Consumers that rely on older assumptions about the contract surface should review the current method list before building or updating clients.
- Wraps remain non-transferable and revoke semantics are not implemented. Frontends and indexers should use contract queries such as `get_wrap`, `balance_of`, and `verify_data` rather than inferring state from events alone.

### Notes for future releases

For every future entry, include a `Migration notes` subsection describing any breaking change and the required updates for client code, backend signers, and indexers.


