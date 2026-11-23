# Reference signer service

`services/signer/` is a reference backend that produces the Ed25519 signature
required by `mint_wrap`. It signs

```
MINT_DOMAIN_SEPARATOR || version || user || period || archetype || data_hash || valid_until
```

matching `crates/wrap-crypto/src/signature.rs` and `docs/signing-payload.md`.

## API

`POST /sign-mint`

```json
{
  "user": "<hex 32-byte account key>",
  "period": 12,
  "archetype": 3,
  "data_hash": "<hex, ignored>",
  "wrap_json": "<canonical indexed wrap JSON>"
}
```

Response:

```json
{
  "signature": "<hex 64-byte Ed25519 signature>",
  "data_hash": "<hex 32-byte recomputed hash>",
  "valid_until": "<unix seconds>"
}
```

The signer **recomputes** `data_hash` from `wrap_json` and never trusts the
client-supplied `data_hash`. `valid_until` is always `now + 120s`. Requests are
rate limited per user (5 / minute) and every signature is written to an
append-only audit log (metadata only — never key material).

## Key management

The service loads the signing key through a `KeyProvider` interface. Production
deployments must supply a KMS/HSM-backed provider. The bundled
`LocalFileKeyProvider` reads a 32-byte hex seed from `SIGNER_KEY_FILE` and is
**for local development only**.

## Deployment

1. Provision the signing key in your KMS/HSM and grant the signer service
   `sign` permission only.
2. Implement `KeyProvider` against your KMS/HSM and wire it in `index.ts`.
3. Set `PORT` and deploy behind TLS. Do not set `SIGNER_KEY_FILE` in prod.
4. Verify signatures against `crates/wrap-crypto/vectors.json` in CI.

## Key rotation

Follow `docs/admin-rotation.md`. In short: publish the new admin key on-chain,
switch the KMS key version, keep the old key available until all in-flight
payloads (TTL 120s) have expired, then revoke it. Audit logs record the
`valid_until` of every signature so rotation windows are auditable.
