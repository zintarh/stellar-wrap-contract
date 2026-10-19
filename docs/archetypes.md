# Archetypes

`mint_wrap` accepts an `archetype` argument typed as a Soroban `Symbol`.

## Symbol constraints

A Soroban `Symbol` is restricted to the character set `[a-zA-Z0-9_]` and may be
at most 32 characters long. Display names that contain spaces or other
characters (for example `The Yield Farmer`) **cannot** be encoded as a `Symbol`
and will fail at argument conversion with an opaque error.

Frontends must map display names to canonical identifiers before calling the
contract. The canonical, versioned list lives in
[`schema/archetypes.json`](../schema/archetypes.json):

| id              | display name      |
| --------------- | ----------------- |
| `yield_farmer`  | The Yield Farmer  |
| `diamond_hands` | Diamond Hands     |
| `degen`         | The Degen         |
| `hodler`        | The Hodler        |
| `whale`         | The Whale         |
| `paper_hands`   | Paper Hands       |
| `builder`       | The Builder       |
| `trader`        | The Trader        |

## Validation decision

The contract does **not** enforce an on-chain allow-list of archetypes; it
accepts any value that is a valid `Symbol` (see the closed proposal #38). This
keeps the contract forward-compatible as new archetypes are added off-chain.
The published list above is the canonical source of truth for clients, and a
test asserts that every id round-trips as a valid `Symbol`.
