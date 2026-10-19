# Architecture v2

This document records architectural decisions for the Stellar Wrap contract repository.

## Frontend inside the contract repo

**Decision (issue #982): Option A — rename `frontend/` to `examples/admin-console`.**

The Vite + React app previously living in `frontend/` (`stellar-wrap-dapp`) is not the product frontend. The product frontend lives in `zintarh/stellar-wrap-frontend` (Next.js). To avoid two competing UIs with divergent contract bindings, the in-repo app is scoped down and relocated:

- **Location:** `examples/admin-console/` (renamed from `frontend/`).
- **Purpose:** an admin and operator console for privileged contract actions — pausing/unpausing, role management, and TTL renewal.
- **Audience:** contract maintainers and operators, not end users. End-user UI work belongs in `zintarh/stellar-wrap-frontend`.
- **Contract bindings:** the console must consume the generated `@stellar-wrap/contract-client` package rather than hand-maintained bindings, so both frontends share a single source of truth for contract types and calls.

Rationale: keeping a small, clearly-scoped admin console in the contract repo is useful for operators exercising privileged paths, while removing ambiguity about where product UI work belongs. Option B (deleting the app) was rejected because the admin/operator flows (pause, roles, TTL renewal) have no home in the product frontend today.

See `examples/admin-console/README.md` for the app's stated purpose and audience.
