# Mainnet Release Checklist

Use this checklist before deploying the Stellar Wrap Contract to mainnet. The mechanical steps are automated; this checklist covers only what needs a person. It complements the release workflow in [.github/workflows/release.yml](.github/workflows/release.yml) and the usage notes in [README.md](README.md).

## 1. Pre-flight validation (automated)

The release-gate workflow runs the pre-flight checks automatically on the release tag:

- Full test suite (`cargo test`).
- Optimized WASM build (`cargo build --release --target wasm32-unknown-unknown`).
- Changelog updated against the tag.

- [ ] Confirm the release-gate workflow passed for the intended release tag before proceeding.

## 2. Release artifact verification (automated)

Artifact verification is automated by `scripts/verify-release-artifact.sh`, which downloads the release artifact and its SHA256 file, verifies the hash, and compares it against the value published in the release body.

- [ ] Run the verification script against the intended release tag:
  - [ ] `scripts/verify-release-artifact.sh <tag>`
- [ ] Confirm the script reports a successful verification.

## 3. Configuration and signer readiness (human)

- [ ] Confirm the intended admin address and admin public key for mainnet initialization.
- [ ] Confirm the correct mainnet network passphrase, RPC endpoint, and fee configuration.
- [ ] Confirm the deployment source account has sufficient funding and is the correct account for the production deployment.
- [ ] Back up all relevant private keys or recovery material offline and store them in a secure location.
- [ ] Verify that the signer used for initialization is available and authorized for the deployment transaction.

## 4. Initialization (human)

- [ ] Prepare the initialization payload with the final admin and admin public key values.
- [ ] Submit the `initialize(admin, admin_pubkey)` transaction signed by the intended admin account, only after the artifact and configuration checks above are complete.
- [ ] Verify the contract instance is initialized successfully and that the admin address is set as expected.
- [ ] Record the deployed contract ID and the final initialization parameters for operational reference.

## 5. Rollback or redeploy notes

- [ ] If initialization fails, stop and do not proceed to any mint or admin actions.
- [ ] Treat a failed initialization as an incomplete deployment; do not reuse a failed instance for production.
- [ ] If the transaction failed before the contract was initialized, redeploy a fresh contract instance with the verified WASM artifact and corrected parameters.
- [ ] If the deployment was submitted but the initialization transaction did not finalize, confirm the network status before redeploying and keep the original deployment details for audit purposes.

## 6. Final sign-off (human)

- [ ] Confirm the release artifact, hash, admin keys, signer backups, and initialization output were reviewed by the responsible operator.
- [ ] Record the date, operator, and deployed contract ID in the release notes or deployment log.
