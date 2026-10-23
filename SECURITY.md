# Security Policy

The Stellar Wrap Contract project takes the security of our smart contracts, user assets, and decentralized infrastructure seriously. We appreciate the efforts of security researchers and community members who identify and responsibly disclose vulnerabilities.

This document outlines our supported versions, vulnerability reporting process, response service level agreements (SLAs), scope, and safe harbor policy.

## Supported Versions

Only active development branches and recent releases receive security fixes. Older tags are not patched; upgrade to the current release instead.

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| main    | :white_check_mark: |
| < 0.1   | :x:                |

When a security vulnerability is identified and patched:
- Fixes are applied to the `main` branch.
- Patch releases are tagged on the current release line (`0.1.x`).
- Integrators are notified via GitHub Security Advisories and release notes.

## Reporting a Vulnerability

**Please do NOT file public GitHub issues, discussions, or pull requests for security vulnerabilities.** Public disclosure exposes contract users to potential exploit before a remediation can be deployed.

### Primary Channel: GitHub Security Advisories (Private Vulnerability Reporting)

The preferred and most secure way to report a vulnerability is through GitHub's Private Vulnerability Reporting mechanism:

1. Navigate to the repository on GitHub: [`zintarh/stellar-wrap-contract`](https://github.com/zintarh/stellar-wrap-contract).
2. Click on the **Security** tab at the top of the repository.
3. Select **Advisories** under "Security overview" or click **Report a vulnerability** directly:
   - Direct link: [Report a vulnerability](https://github.com/zintarh/stellar-wrap-contract/security/advisories/new)
4. Fill in the report with as much detail as possible (see [What to Include](#what-to-include-in-a-report) below).
5. Submit the advisory. This opens a private, encrypted collaboration workspace between you and the repository maintainers.

### Fallback Reporting Channel

If GitHub Security Advisories are unavailable or you encounter issues submitting via the web interface:

- Contact maintainers privately via email: `security@zintarh.com`
- If transmitting sensitive details, please request our PGP public key or encrypt the communication.

### What to Include in a Report

To help us triage and resolve the issue quickly, please include:

- **Summary & Vulnerability Type**: A clear description of the vulnerability (e.g., unauthorized state mutation, signature replay, overflow/underflow, authorization bypass, reentrancy/invocation griefing, timelock evasion, bridge inconsistency).
- **Affected Components**: Specific contracts, source files (`src/`, `contracts/`), functions, or entrypoints (for example `src/signature.rs` or a `lib.rs` entrypoint).
- **Reproduction Steps / Proof of Concept (PoC)**: Step-by-step reproduction instructions, transaction sequences, or test code (such as a Rust unit test using `soroban-sdk::testutils` or standalone test script) demonstrating the vulnerability.
- **Expected Impact & Severity**: Your assessment of the impact (fund loss, unauthorized mint, denial of service, data exposure) and severity level (Critical, High, Medium, Low).
- **Deployment Details**: The network and contract ID or WASM hash, if tested against a deployment.
- **Potential Mitigations**: Any suggested fixes, defensive assertions, or mitigation strategies (if available).

## Response SLA

We commit to handling security reports professionally and expeditiously under the following service level agreements:

| Stage | Target Timeline | Details |
| ----- | --------------- | ------- |
| **Initial Acknowledgement** | Within 24–48 hours (max 5 business days) | Confirmation that the report was received and assigned to a maintainer for triage. |
| **Triage & Assessment** | Within 3–5 business days | Reproduction of the issue, severity assessment, and verification of whether the finding is in scope. |
| **Status Updates** | Every 7 calendar days | Regular progress updates on investigation, impact analysis, and remediation development. |
| **Fix Development & Testing** | Within 14–30 calendar days (varies by severity) | Developing, reviewing, and validating patches against invariant, integration, and fuzz test suites in a private advisory fork. |
| **Coordinated Disclosure** | Up to 90 calendar days | Coordinated release of the fix and publication of the advisory, allowing adequate time for mainnet/testnet deployment. |

If an actively exploited vulnerability is reported, these timelines will be accelerated accordingly.

## Scope

### In-Scope Contracts and Components

The following smart contracts and codebases within this repository are in scope:

- **Core Stellar Wrap Contract (`stellar_wrap_contract`)**:
  - Source code located in `src/`.
  - Modules: `admin.rs`, `bridge.rs`, `burn.rs`, `governance.rs`, `guard.rs`, `lib.rs`, `merkle.rs`, `mint.rs`, `optout.rs`, `oracle.rs`, `queries.rs`, `revoke.rs`, `signature.rs`, `stake.rs`, `storage_accounting.rs`, `storage_types.rs`, `timelock.rs`, `token.rs`, `transfer.rs`, `ttl.rs`.
  - Core functionalities: wrap record minting, non-transferable soulbound validation, period verification, Ed25519 payload signature checks, bridge transfers/refunds, role governance, emergency pause mechanics, and timelocked actions.
- **Staking Extensions (`contracts/staking/`)**:
  - Source code located in `contracts/staking/`.
  - Staking mechanisms, lockups, and reward accounting.
- **Deployment and Release Artifacts**:
  - Release artifacts built by `.github/workflows/release.yml`.
  - Critical scripts located in `scripts/` directly governing release integrity (e.g., release artifact hash verification, WASM size bounds).
- **Signing and Verification Flows**:
  - Specifications and workflows documented in `docs/` (such as `docs/signing-payload.md` and `docs/adr/0001-in-guest-ed25519.md`).

### Out of Scope

The following areas and vulnerability classes are outside the scope of this security policy:

- **Known Public Issues**: Issues already tracked in the repository with the `security` label (e.g., issues documented in `docs/security/security-audit-checklist.md` and `docs/security/security-recommendations.md`, such as #647, #650, #651, #653, #672).
- **Underlying Protocol and Dependencies**: Vulnerabilities originating in third-party crates, upstream Soroban SDK (`soroban-sdk`), Soroban host runtime (`soroban-env-host`), or Stellar Core/Stellar network consensus, unless directly caused by incorrect configuration or usage in this repository. Third-party dependencies with no reachable path in this contract should be reported upstream.
- **Attacks Requiring Privileged Key Compromise**: Compromise of administrator private keys or physical theft of signers/machines (while defense-in-depth timelock/governance improvements are welcomed, simple administrative misuse with valid credentials is out of scope).
- **Automated Scanner Reports**: Reports generated purely by automated scanners without a demonstrated, reproducible impact.
- **Social Engineering & Phishing**: Phishing, social engineering, or physical attacks targeting developers, maintainers, or users.
- **Volumetric Denial of Service**: Spamming public Stellar RPC endpoints or transactions to exhaust node resources without exploiting contract algorithmic complexity.
- **Non-Security Issues**: Style guidelines, documentation typos, formatting, and test-only helper utilities that do not impact deployed bytecode.

## Severity Classification

We classify vulnerabilities according to their potential impact on smart contract security and user funds:

| Severity | Description | Examples |
| -------- | ----------- | -------- |
| **Critical** | Direct theft, permanent freezing, or unauthorized minting of assets/records with no special privileges required. | Unauthorized wrap minting without valid signature; draining locked bridge funds; arbitrary state corruption. |
| **High** | Conditional or temporary loss of funds, unauthorized state changes under specific edge conditions, or complete denial of service of contract operations. | Bypassing timelock delays; bricking contract upgradeability; replaying signatures across chains or instances. |
| **Medium** | State inconsistencies, griefing attacks requiring disproportionate attacker capital, or circumvention of transfer fees/opt-out states. | Inaccurate storage accounting leading to premature eviction; fee calculation truncation; partial event silence. |
| **Low** | Minor logic discrepancies, non-critical event payload discrepancies, or theoretical edge cases with negligible impact. | Informational parameter validation; minor gas/storage inefficiencies without unbounded loops. |

## Safe Harbor

We consider security research conducted in accordance with this policy to be authorized. We will not initiate legal action against researchers who make a good-faith effort to comply with the following principles:

- **Responsible Discovery**: Conduct research within the scope described above and avoid violating privacy, degrading system performance, or destroying data.
- **No Exploitation**: Do not exploit identified vulnerabilities on live networks beyond what is necessary to prove the security impact in a controlled test environment (e.g., local Soroban sandbox or testnet). Do not seize or drain user assets.
- **Confidentiality**: Give maintainers reasonable time to investigate and remediate vulnerabilities before sharing information publicly or with third parties.
- **Compliance**: Act in good faith and comply with all applicable local and international laws.

If you follow these guidelines, we will work with you to understand and address the issue, and we will publicly acknowledge your contribution in the advisory (unless you prefer to remain anonymous).

## Security Documentation

- [Security recommendations](docs/security/security-recommendations.md)
- [Security audit checklist](docs/security/security-audit-checklist.md)
- [Mainnet release checklist](docs/operations/mainnet-release-checklist.md)

## Maintainer Task: Repository Configuration

To activate private vulnerability reporting on GitHub:

1. Go to the repository **Settings**.
2. Under the **Security** section in the sidebar, select **Code security and analysis**.
3. Under **Private vulnerability reporting**, click **Enable**.
4. Confirm that the security policy appears under the **Security** tab of the repository.
