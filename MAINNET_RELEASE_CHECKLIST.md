# Mainnet Release Checklist

Use this checklist before deploying the Stellar Wrap Contract to mainnet. It complements the release workflow in [.github/workflows/release.yml](.github/workflows/release.yml) and the usage notes in [README.md](README.md).

## 1. Pre-flight validation

- [ ] Confirm the intended release tag is the one you are deploying and that the changelog is updated.
- [ ] Run the full test suite locally:
  - [ ] `cargo test`
- [ ] Build the optimized WASM artifact locally:
  - [ ] `cargo build --release --target wasm32-unknown-unknown`

## 2. Release artifact verification

- [ ] Download the GitHub release artifact named `stellar_wrap_contract.wasm` from the release page.
- [ ] Download the matching SHA256 file named `stellar_wrap_contract.wasm.sha256` from the same release.
- [ ] Verify the downloaded artifact hash matches the published SHA256 file:
  - [ ] `sha256sum stellar_wrap_contract.wasm`
  - [ ] Compare the output to the contents of `stellar_wrap_contract.wasm.sha256`
- [ ] Confirm the artifact hash matches the value published in the GitHub release body.

## 3. Configuration and signer readiness

- [ ] Confirm the intended admin address and admin public key for mainnet initialization.
- [ ] Confirm the correct mainnet network passphrase, RPC endpoint, and fee configuration.
- [ ] Confirm the deployment source account has sufficient funding and is the correct account for the production deployment.
- [ ] Back up all relevant private keys or recovery material offline and store them in a secure location.
- [ ] Verify that the signer used for initialization is available and authorized for the deployment transaction.

## 4. Initialization

- [ ] Prepare the initialization payload with the final admin and admin public key values.
- [ ] Submit the `initialize(admin, admin_pubkey)` transaction signed by the intended admin account, only after the artifact and configuration checks above are complete.
- [ ] Verify the contract instance is initialized successfully and that the admin address is set as expected.
- [ ] Record the deployed contract ID and the final initialization parameters for operational reference.

## 5. Rollback or redeploy notes

- [ ] If initialization fails, stop and do not proceed to any mint or admin actions.
- [ ] Treat a failed initialization as an incomplete deployment; do not reuse a failed instance for production.
- [ ] If the transaction failed before the contract was initialized, redeploy a fresh contract instance with the verified WASM artifact and corrected parameters.
- [ ] If the deployment was submitted but the initialization transaction did not finalize, confirm the network status before redeploying and keep the original deployment details for audit purposes.

## 6. Final sign-off

- [ ] Confirm the release artifact, hash, admin keys, signer backups, and initialization output were reviewed by the responsible operator.
- [ ] Record the date, operator, and deployed contract ID in the release notes or deployment log.
