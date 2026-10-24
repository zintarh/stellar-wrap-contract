# Architecture Decision Records (ADRs+

This directory holds the Architecture Decision Records (ADRs+) for Stellar Wrap. ADRs capture the *why* behind significant technical decisions so that rationale remains discoverable after the people who made the decision have moved on.

The v2 rewrite will produce many decisions. Recording them here keeps the architecture history together in one place instead of scattered across ad-hoc files.

## Index

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-in-guest-ed25519-signature-verification.md) | In-guest Ed25519 signature verification | Accepted |
| [0002](0002-proxy-pattern.md) | Proxy pattern for contract interaction | Accepted |
| [0003](0003-contract-split.md) | Contract split | Accepted |
| [0004](0004-rbac.md) | Role-based access control (RBAC) | Accepted |
| [0005](0005-userindex.md) | UserIndex | Accepted |
| [0006](0006-fsm-reduction.md) | Finite state machine reduction | Accepted |
| [0007](0007-transfer-decision.md) | Transfer decision | Accepted |

## Conventions

- Files are named `NNNN-short-title.md` with a zero-padded, monotonically increasing number.
- New ADRs are added by copying [`template.md](template.md) and assigning the next available number.
- ADRs are immutable once accepted. If a decision changes, add a new ADR that supersedes the old one and update the old ADR status to `Superseded by NNNN`.
- Use the status values `Proposed`, `Accepted`, `Rejected`, `Deprecated`, or `Superseded by NNNN`.
- Keep each ADR to one decision. List alternatives considered and the trade-offs in the Consequences section.

## Template

Every ADR follows the structure in [`template.md](template.md):

- `Context` - the forces at play and the problem being solved.
- `Decision` - the change we have agreed to make.
- `Consequences` - the resulting trade-offs, including what becomes easier and what becomes harder.
- `Status` - the current state of the decision.

## References

- [Architecture characteristic records](https://github.com/joeparison/adr-tools)
- [DOCUMENTING ARCHITECTURE DECISIONS](https://adr.github.io/)
