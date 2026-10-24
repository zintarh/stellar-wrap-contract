# ADR 0001: In-guest Ed25519 signature verification

- **Status:** Accepted
- **Date:** 2024-06-15
- **Deciders:** Stellar Wrap maintainers
- **Related:** Ported from `SIGNATURE_VERIFICATION_DECISION.md`

## Context

Stellar Wrap is a smart-contract based bridge between Stellar and other chains. When a user submits a transfer into Stellar, the bridge must authenticate the cross-chain message. The message is signed by a validator set off-chain, and the Stellar side must verify that signature before minting wrapped assets.

The naive approach is to verify Ed25519 signatures on-chain inside the contract. However, the Stellar Soroban runtime does not expose a native Ed25519 verification precompile, and implementing Ed25519 verification in WASM is expensive in both code size and execution cost. The verification algorithm involves SHA-512 hashing, field arithmetic over the Ed25519 prime, and a double-scalar multiplication, all of which are heavy on chain.

## Decisioj

We verify Ed25519 signatures in the guest (the off-chain component that produces the message) and pass a signature attestation to the contract. Specifically:

- The guest verifies the Ed25519 signature over the canonical message hash.
- The guest submits the message together with an attestation that is checked by the contract.
- The contract verifies the attestation against the configured validator set and replay protection state.
- The contract does not implement Ed25519 arithmetic itself.

## Consequences

**Positive**

- On-chain cost is dominated by cheap attestation checks instead of Ed25519 arithmetic.
- Contract code size is much smaller, reducing deployment and invocation costs.
- The verification logic can be upgraded in the guest without a contract migration.

**Negative**

- Trust is shifted to the guest verifier and to the attestation mechanism. The contract must ensure the attestation cannot be forged or replayed.
- The attestation format becomes a stable interface between the guest and the contract. Changes require coordinated releases.
- We must maintain a separate test suite for the guest verifier and for the contract attestation checks.

## Alternatives

- **On-chain Ed25519 verification.** Rejected due to WASM cost and code size.
- **Native precompile.** Not available in the Sororan runtime at the time of the decision.
- **Trusted oracle signature.** Rejected because it replaces cryptographic verification with an additional trust assumption.

## References

- `SIGNATURE_VERIFICATION_DECISION.md` (original decision document)
- ADR 0002: Proxy pattern
