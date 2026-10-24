# ADR 0003: Split Contracts by Responsibility

- Status: Accepted
- Date: 2025-01-15
- Deciders: Stellar Wrap v2 architecture team
- Consulted: Security and contract maintainers
- Informs: Contract layout, deployment scripts, and client SDKs

## Context

The v1 implementation grew into a single large contract that handled
user registration, asset custody, transfer logic, and administrative
controls. This created several problems:

- **Audit surface**: every change touched a contract that held user funds,
  forcing a full security review even for non-financial changes.
- **Upgrade risk**: a single deploy could inadvertently alter balance
  accounting or admin policies.
- **Testing coupling**: tests for user metadata required funding and
  admin setup, making unit tests slow and brittle.
- **Bloated WASM**: every deployment carried code for features that a
  given deployment might not use.

The v2 rewrite is an opportunity to separate concerns along clear
ownership boundaries.

## Decision

We will split the system into multiple purpose-built contracts,
each with a narrow, well-defined responsibility:

1. **UserIndex** - registration and lookup of user identities and
   public keys. Holds no funds.
2. **Account** - per-user asset custody and balance accounting. One
   contract instance per user (or logical account), owned by the user's
   authority.
3. **Transfer*** - coordination of movements between accounts,
   including validation and event emission.
4. **Admin** - governance configuration, fee policies, and
   pause/resume controls.

The contracts communicate through explicit, versioned interfaces. The
*proxy pattern* (ADR-0002) is used where a stable entry point must
forward to an upgradeable implementation.

## Consequences

### Positive

- **Smaller audit surface**: each contract can be audited in isolation.
  Changes to UserIndex do not require a funds-handling review.
- **Independent upgrades**: the Admin contract can evolve without
  redeploying Account or Transfer logic.
- **Testability**: each contract has focused unit tests and can be
  mocked at the interface boundary.
- **Clearer ownership**: teams and contributors can own a single
  contract without needing context on the entire system.

### Negative

- **More deployments**: each contract must be built, deployed, and
  versioned. Tooling must handle multiple artifacts and their
  dependencies.
- **Cross-contract calls**: transfers involve multiple contracts, adding
  latency and failure modes that must be handled atomically or with
  compensation.
- **Interface discipline**: changes to shared interfaces require
  coordination across contracts and clients.
- **Increased complexity**: developers must understand the boundaries
  and the call graph.

### Mitigations

- **Versioned interfaces**: each cross-contract call goes through a
  declared interface with an explicit version, allowing gradual
  migration.
- **Integration tests**: an end-to-end suite exercises the full call
  graph against deployed contracts.
- **Shared library**: common types and auth helpers live in a
  shared library crate to avoid drift.

## Alternatives Considered

- **Monolithic contract**: simpler deployment but retains the v1
audit and upgrade problems.
- **Single contract with feature flags**: reduces deployments but
  keeps all code in the audit surface and does not separate
  ownership.
- **Separate contracts per asset**: multiplies deployments without
  adding meaningful isolation beyond the Account boundary.

## References

- ADR-0001: In-guest Ed25519 signature verification.
- ADR-0002: Proxy pattern for upgradeable contracts.
- ADR-0004: Role-based access control.
- ADR-0005: UserIndex design.
- ADR-0006: FSM reduction.
- ADR-0007: Transfer decision.
