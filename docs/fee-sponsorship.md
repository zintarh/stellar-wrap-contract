# Fee Sponsorship for `mint_wrap`

New users often have little or no XLM, which blocks them from paying transaction
fees and storage rent required to call `mint_wrap`. Stellar's **fee-bump
transaction** mechanism lets a sponsor pay those costs on a user's behalf
without gaining any contract authority.

## How it works

A Stellar fee-bump transaction wraps an existing signed transaction (the "inner
transaction") in an outer envelope signed by the sponsor. The network charges
fees and storage rent to the **outer envelope's source account** (the sponsor),
not to the inner transaction's source account (the user).

From the contract's point of view, nothing changes:
- `mint_wrap` calls `user.require_auth()` — this checks whether the **user
  address signed the Soroban auth entry**, not who paid for the transaction.
- The fee-bump source account is invisible to Soroban contract logic.
- No contract changes are required to enable sponsorship.

### Key insight

Soroban authentication (`require_auth`) is separate from Stellar transaction
fees. The user must sign the inner transaction's auth entry; the sponsor only
signs the outer fee-bump envelope. These are cryptographically independent.

## Step-by-step flow

```
┌─────────────────────────────────────────────────────────┐
│  1. Backend signs the mint payload (Ed25519 admin key)   │
│     and returns `signature` to the frontend.             │
└─────────────────────────┬───────────────────────────────┘
                           │
┌─────────────────────────▼───────────────────────────────┐
│  2. Frontend builds an inner transaction:                │
│       source = user account                              │
│       operation = InvokeContractOp(mint_wrap, ...)       │
│     Assembles auth entries via simulateTransaction.      │
└─────────────────────────┬───────────────────────────────┘
                           │
┌─────────────────────────▼───────────────────────────────┐
│  3. User signs the inner transaction (e.g. Freighter).   │
│     This satisfies user.require_auth() inside the        │
│     contract.                                            │
└─────────────────────────┬───────────────────────────────┘
                           │
┌─────────────────────────▼───────────────────────────────┐
│  4. Sponsor wraps the signed inner XDR in a fee-bump     │
│     transaction and signs the outer envelope.            │
│       fee_bump source  = sponsor account                 │
│       base_fee         = sponsor's chosen fee            │
└─────────────────────────┬───────────────────────────────┘
                           │
┌─────────────────────────▼───────────────────────────────┐
│  5. Sponsor submits the fee-bump envelope.               │
│     The network deducts fees from the sponsor.           │
│     Contract reads only the user auth entry — it never   │
│     sees the sponsor's key.                              │
└─────────────────────────────────────────────────────────┘
```

## Contract compatibility

`src/mint.rs` only uses `user.require_auth()`. There are no calls to
`e.source_account()` anywhere in `mint_wrap` or `mint_wrap_batch`, so neither
function has any dependency on who submitted the transaction. The contract is
fully compatible with fee-bump sponsorship without any code changes.

## TypeScript / SDK example

See `frontend/src/lib/stellar.ts` for the complete `mintWrapSponsored`
function. The abbreviated pattern is:

```typescript
import {
  FeeBumpTransaction,
  FeeBumpTransactionBuilder,
  Keypair,
  Transaction,
} from "@stellar/stellar-sdk";

/**
 * Mint a wrap record where a sponsor pays the transaction fee.
 *
 * @param innerXdr  - Signed inner transaction XDR (signed by the user via Freighter)
 * @param sponsorKeypair - Sponsor keypair that covers the fee
 * @param networkPassphrase - Network passphrase
 * @param maxFee   - Maximum fee in stroops the sponsor is willing to pay
 */
function buildFeeBumpEnvelope(
  innerXdr: string,
  sponsorKeypair: Keypair,
  networkPassphrase: string,
  maxFee: string = "1000000", // 0.1 XLM
): FeeBumpTransaction {
  const inner = new Transaction(innerXdr, networkPassphrase);
  return new FeeBumpTransactionBuilder({
    feeSource: sponsorKeypair.publicKey(),
    baseFee: maxFee,
    innerTransaction: inner,
    networkPassphrase,
  })
    .setTimeout(30)
    .build();
}
```

The full helper in `stellar.ts` handles:
1. Building the inner transaction (same as the normal `mintWrap` flow)
2. Simulating and assembling Soroban auth entries
3. Signing the inner transaction with Freighter (user's auth)
4. Wrapping in a fee-bump envelope signed by the sponsor
5. Submitting and polling for confirmation

## Stellar CLI example

If you have the sponsor's secret key available locally, you can test this with
the Stellar CLI:

```bash
# 1. Build the inner transaction (user is source)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source <USER_SECRET> \
  --build-only \
  -- mint_wrap \
  --user <USER_ADDRESS> \
  --period 202406 \
  --archetype builder \
  --data_hash <DATA_HASH_HEX> \
  --payload_version 1 \
  --signature <SIGNATURE_HEX> \
  > inner.xdr

# 2. Wrap in a fee-bump and submit
stellar tx fee-bump \
  --fee-account <SPONSOR_ADDRESS> \
  --source <SPONSOR_SECRET> \
  --network testnet \
  --inner-transaction "$(cat inner.xdr)"
```

The `--source` on the outer `fee-bump` command corresponds to the sponsor
paying the fee. The user's inner transaction authorizes the contract call.

## Integration test

`tests/integration_testnet.sh` includes a fee-bump section (step 9) that:

1. Derives a one-time sponsor account from `STELLAR_SPONSOR_SECRET`
2. Builds the inner `mint_wrap` transaction with the user as source
3. Wraps it in a fee-bump transaction signed by the sponsor
4. Submits and asserts the wrap was recorded under the user's address

Run with:

```bash
export STELLAR_DEPLOYER_SECRET=S...
export STELLAR_ADMIN_PUBKEY=aabb...    # 64 hex chars
export STELLAR_MINT_SIGNATURE=ccdd...  # 128 hex chars
export STELLAR_SPONSOR_SECRET=S...     # sponsor pays the fee
bash tests/integration_testnet.sh
```

## Security considerations

- **The sponsor has no contract authority.** They cannot mint, revoke, or read
  any data on behalf of the user. They only pay fees.
- **Replay protection is unchanged.** Each `mint_wrap` signature is bound to
  the user address and period via the canonical payload; a sponsored mint
  cannot be replayed for a different user.
- **Storage rent.** Persistent entry rent is also charged to the fee-bump
  source account (the sponsor), not the user. This means a sponsor covers both
  the transaction fee and the initial storage rent for the user's wrap record.
- **Sponsor accountability.** Only give sponsorship capability to trusted
  backend services. A compromised sponsor can spam fee-bump transactions but
  cannot mint wraps without valid admin signatures.
