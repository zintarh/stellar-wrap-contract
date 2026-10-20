# Contract upgrade policy

## Selected v2 policy: governor plus timelock

The v2 registry and every upgradeable satellite (including token and bridge
contracts) will remain upgradeable, but no externally held admin key may
upgrade them directly. Each contract's admin authority must be a governed
controller, and the controller must route every upgrade through that
contract's enabled timelock. A proposal alone does not change code: it must
pass the governor's configured voting rules, be queued, wait the published
timelock delay, and then be executed. No v2 contract is planned to have a
`freeze_upgrades` switch under this policy.

| Contract | Upgrade authority | Required path | Version/event |
| --- | --- | --- | --- |
| Registry | Governor-controlled admin | Governor approval, then registry timelock | Increment that registry's `contract_version`; emit the new version and WASM hash |
| Token satellite | Governor-controlled admin | Governor approval, then token timelock | Independent version counter and upgrade event |
| Bridge satellite | Governor-controlled admin | Governor approval, then bridge timelock | Independent version counter and upgrade event |
| Other v2 satellites | Governor-controlled admin | Governor approval, then that satellite's timelock | Independent version counter and upgrade event |

Each contract has its own upgrade queue and version counter. A timelock on one
satellite does not authorize or delay upgrades to another. Deployment is not
considered compliant until the governor contract is installed as the admin
and the timelock is enabled; an EOA or multisig may not retain a parallel
direct-upgrade route.

## Current repository implementation

This repository currently contains the pre-split registry, not the planned v2
contract set. Its `upgrade` entrypoint is admin-authorized. Once
`enable_timelock` is called, direct upgrades are rejected and the scheduled
`TimelockAction::Upgrade` route is required. The current governance module only
votes on admin changes; it does not approve arbitrary WASM upgrades. Therefore
the v2 governor-to-timelock integration and governor voting rules remain a
deployment prerequisite for the v2 rewrite, not a capability of this legacy
contract.

Both direct and timelocked upgrades in this repository now use the same
upgrade helper. Each increments `contract_version` exactly once and emits an
`upgrade` event containing the version and new WASM hash. Storage schema
versioning remains separate and must be migrated deliberately when a WASM
change alters persisted layouts.

## Operational requirements

- Publish each contract's governor, admin, and timelock addresses, voting
  parameters, and delay before deployment.
- Require the configured minimum timelock delay for all upgrade proposals.
- Test the governor approval, scheduling, delay, execution, version increment,
  and event for every contract independently.
- Retain no direct EOA upgrade path after the v2 governor is installed.
- Review an upgrade's storage migration and compatibility plan before it is
  submitted to the governor.
