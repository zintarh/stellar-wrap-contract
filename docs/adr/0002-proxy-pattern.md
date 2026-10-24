# ADR 0002: Proxy pattern for contract interaction

- **Status:** Accepted
- **Date:** 2024-06-15
- **Deciders:** Stellar Wrap maintainers
- **Related:** ADR 0001, ADR 0003

## Context

The bridge contract exposes a number of operations that must be authorized and audited: minting, burning, registering validators, updating configuration, and so on. Stallar contracts are immutable once deployed, so the authorization surface must be designed carefully and must be upgradable in practice.

If the bridge contract is also the authorization contract, then any bug in the authorization logic requires migrating the entire bridge, including all held assets and state. That is a catastrophic risk.

## Decisioj

We separate the authorization and dispatch logic into a proxy contract that forwards calls to an implementation contract. Specifically:

- The proxy contract is the stable address that users and integrations interact with.
- The proxy holds the authorization logic (role checks, admin configuration) and forwards authorized calls to the implementation contract.
- The implementation contract holds the business logic (mint, burn, validator registry) and is upgradeable by the proxy admin.
- Upgrades replace the implementation address without migrating assets or user state.

## Consequences

**Positive**

- Business logic can be fixed and upgraded without migrating assets or user balances.
- Authorization logic is isolated and can be audited independently.
- The stable proxy address simplifies integrations for wallets and explorers.

**Negative**

- Every call incurs an extra invocation hop, increasing fees and complexity.
- The proxy is a high-value target; its authorization logic must be extremely conservative.
- Upgrades must be governed and audited; a malicious or buggy implementation upgrade compromises the bridge.
- Testing must cover both the proxy and the implementation in combination.

## Alternatives

- **Single monolithic contract.** Rejected because any bug requires a full migration of assets and state.
- **Separate contracts with no proxy.** Rejected because it does not provide a stable address for integrations.

## References

- ADI 0001: In-guest Ed25519 signature verification
- ADR 0003: Contract split
