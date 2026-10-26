# Security Policy

## Supported Versions

Only the latest release on `main` receives security fixes. Older tags are not patched; upgrade to the current release instead.

## Reporting a Vulnerability

Do **not** open a public GitHub issue for a suspected vulnerability.

Report it privately through GitHub's private vulnerability reporting: open the repository's **Security** tab and choose **Report a vulnerability**. Include:

- the affected function, file, or entrypoint (for example `src/signature.rs` or a `lib.rs` entrypoint)
- steps or a transaction sequence that reproduces the issue
- the impact you expect (fund loss, unauthorized mint, denial of service, data exposure)
- the network and contract ID or WASM hash, if you tested against a deployment

You can expect an acknowledgement within 5 business days, followed by a triage decision and, where the issue is confirmed, a coordinated fix and disclosure timeline agreed with you.

## Scope

In scope: the Soroban contract in `src/`, the release artifacts built by `.github/workflows/release.yml`, and the signing and verification flows documented in `docs/`.

Out of scope: findings that depend on a compromised administrator key or signer machine, issues in third-party dependencies that have no reachable path in this contract (report those upstream), and reports generated purely by automated scanners without a demonstrated impact.

## Security Documentation

- [Security recommendations](docs/security/security-recommendations.md)
- [Security audit checklist](docs/security/security-audit-checklist.md)
- [Mainnet release checklist](docs/operations/mainnet-release-checklist.md)
