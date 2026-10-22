# Threat Model

One-page answer to **"what happens if X is compromised?"** for privileged and
user-facing actors. Authority routes and fastest paths live in
[authority-model.md](authority-model.md). Incident response steps live in
[incident-runbook.md](incident-runbook.md). Do not duplicate those here.

## Assets and trust assumptions

| Asset | Why it matters |
| --- | --- |
| Admin address / governance / timelock | Can pause, upgrade, rotate keys, change bridge and fees — see [authority-model.md](authority-model.md) |
| Backend signing key (`AdminPubKey`) | Authorizes `mint_wrap` / batch mint payloads — see [signing-payload.md](signing-payload.md) |
| Bridge relayer keys | Authorize inbound bridge mint and refunds — see [bridge-architecture.md](bridge-architecture.md) |
| User wrap records & TTLs | User state and persistence; renewal is permissionless for non-terminal wraps |
| Whitelist / fee / pause config | Controls who can mint and whether value can be redirected |

**Trust assumptions (high level):** admin is a protected key or multisig; the
backend signer is offline-HSM/KMS and rate-limited; bridge relayers meet the
configured threshold; users only authorize their own actions; renewal bots are
untrusted but limited to TTL extension of non-terminal wraps.

Related: [SECURITY_AUDIT_CHECKLIST.md](../SECURITY_AUDIT_CHECKLIST.md),
[SECURITY_RECOMMENDATIONS.md](../SECURITY_RECOMMENDATIONS.md),
[SIGNATURE_VERIFICATION_DECISION.md](../SIGNATURE_VERIFICATION_DECISION.md).

---

## Actors (STRIDE-oriented compromise scenarios)

For each actor: primary STRIDE categories, compromise impact, detection hints,
response, and runbook link.

### 1. Backend signer (`AdminPubKey`)

| | |
| --- | --- |
| **STRIDE** | Spoofing, Tampering, Elevation of privilege (unauthorized mint) |
| **If compromised** | Attacker can produce valid mint signatures for arbitrary users/periods (subject to whitelist and pause). Cannot upgrade the contract or change admin by itself. |
| **Detection** | Spike in mint events; signatures issued outside expected rate/audit log; unexpected recipients or periods. |
| **Response** | Pause → rotate signing pubkey (`update_admin_pubkey` / timelock) → revoke bad wraps per [revoke-policy.md](revoke-policy.md) → unpause after verification. |
| **Runbook** | [incident-runbook.md](incident-runbook.md) §1 (pause), §3.1 step 4 (pubkey rotation), §5 checklist |

### 2. Admin / governor

| | |
| --- | --- |
| **STRIDE** | Tampering, Elevation of privilege, Denial of service (pause/config) |
| **If compromised** | Full control via fastest paths in [authority-model.md](authority-model.md): rotate admin, upgrade WASM, change relayers/fees/whitelist, schedule timelock actions. Pause alone does not stop admin-gated recovery paths. |
| **Detection** | Unexpected `update_admin` / `upgrade` / `set_bridge_relayers` / timelock schedule events; sudden fee or whitelist changes. |
| **Response** | Pause immediately → cancel malicious timelock ops if still admin → two-step admin handover → rotate signing pubkey and bridge config → unpause. |
| **Runbook** | [incident-runbook.md](incident-runbook.md) §1–§3, §5–§6; planned rotation in [admin-rotation.md](admin-rotation.md) |

### 3. Bridge relayer

| | |
| --- | --- |
| **STRIDE** | Spoofing, Tampering, Repudiation (false inbound / refund) |
| **If compromised** | Can mint via `bridge_wrap_in` (threshold-dependent) and abuse `bridge_wrap_refund`. Cannot upgrade or change admin. Outbound still needs user auth. |
| **Detection** | Inbound mints without matching source-chain activity; refund spikes; nonce anomalies. |
| **Response** | Pause → rotate relayer set (`set_bridge_relayers`) → review nonces / revoke fraudulent wraps → unpause. |
| **Runbook** | [incident-runbook.md](incident-runbook.md) §2.4, §4 |

### 4. Renewal bot

| | |
| --- | --- |
| **STRIDE** | Denial of service / resource abuse (TTL keep-alive) |
| **If compromised** | Can call permissionless `extend_ttl` / `extend_ttl_batch` for non-terminal wraps only. Cannot mint, transfer, upgrade, or change config. Terminal states are not renewed. |
| **Detection** | Unusual TTL-extension tx volume or fees; keep-alive on unexpected addresses. |
| **Response** | No contract pause required for key theft (bot holds no privileged key). Stop the bot process; optionally ignore spam (non-terminal TTL renewals are the intended surface). |
| **Runbook** | No privileged-key runbook; see TTL notes on `extend_ttl` in contract docs / README Pause & storage sections. Operators still use [incident-runbook.md](incident-runbook.md) §1 if a separate incident coincides. |

### 5. End user

| | |
| --- | --- |
| **STRIDE** | Spoofing (stolen wallet), Information disclosure (local), Repudiation |
| **If compromised** | Attacker can auth as the user: burn/transfer own wraps where allowed, initiate outbound bridge, change aliases/opt-out. Cannot mint without a backend signature, cannot admin or relay. |
| **Detection** | User reports lost key; unexpected `transfer_wrap` / `burn_wrap` / `bridge_wrap_out` from the address. |
| **Response** | User rotates wallet keys off-chain; admin may revoke fraudulent wraps only under [revoke-policy.md](revoke-policy.md). No global pause unless abuse is systemic. |
| **Runbook** | User recovery is wallet-side; protocol response limited to revoke policy and, if needed, [incident-runbook.md](incident-runbook.md) pause for systemic abuse. |

---

## Quick reference: blast radius

| Actor compromised | Unauthorized mint | Steal others' wraps | Upgrade / change admin | Config (fees, whitelist, relayers) |
| --- | --- | --- | --- | --- |
| Backend signer | Yes (`mint_wrap`) | No | No | No |
| Admin / governor | Yes (via pubkey / bridge config) | Yes (after taking control) | Yes | Yes |
| Bridge relayer | Yes (`bridge_wrap_in`) | No | No | No (refund path only) |
| Renewal bot | No | No | No | No (TTL only) |
| End user | No | Only own wraps | No | No |

Detail and response procedures: [incident-runbook.md](incident-runbook.md).
Role routes: [authority-model.md](authority-model.md).
