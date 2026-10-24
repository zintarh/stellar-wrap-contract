# ADR 0006: FSM reduction and transfer decision

- **Status:** Accepted
- **Date:** 2024-07-16
- **Deciders:** Stellar Wrap maintainers
- **Related:** ADR 0003

## Context

The v1 bridge modeled the lifecycle of a cross-chain transfer as a large state machine with many states: initiated, attested, queued, minted, confirmed, failed, refunded, and so on. Many of these states were never reached in practice, and the transitions were hard to reason about. The contract stored per-transfer state that was often only used once, increasing storage costs.

The v2 rewrite needs a simpler model that is easy to audit and cheap to execute. It also needs to decide how regular user transfers (as opposed to bridge transfers) are handled.

## Decisioj

We reduce the bridge FSM to two states and decide that regular user transfers do not go through the bridge FSM. Specifically:

- The bridge FSM has only two states: `Active` and `Paused`. The admin can pause and resume the bridge.
- Per-transfer state is not stored in the bridge contract. Transfers are either completed in a single invocation or rejected.
- Regular user transfers of wrapped assets are handled by the underlying Stellar asset issuer mechanisms and do not involve the bridge contract.
- Bridge transfers (cross-chain in and out) are explicit operations that either succeed or fail atomically.

## Consequences

**Positive**

- The FSM is trivial to audit and test.
- Storage costs drop because per-transfer state is not persisted.
- User transfers are not constrained by bridge logic and can use native Stellar tooling.

**Negative**

- Transfers that fail mid-flight cannot be resumed; they must be retried from scratch.
- The bridge cannot implement complex multi-step flows (e.g., time-locked refunds) without adding state back.
- Off-chain components must handle retries and failure reporting.

## Alternatives

- **Keep the large FSM.** Rejected because most states were unreachable and the complexity outweighed the benefit.
- **Route user transfers through the bridge.** Rejected because it adds cost and attack surface without functional benefit.

## References

- ADR 0003: Contract split
