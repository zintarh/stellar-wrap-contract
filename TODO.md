# TODO

- [x] Implement storage deposit tracking to prevent storage-exhaustion DoS
  - [ ] Add storage budget/config keys to `src/storage_types.rs`
  - [ ] Add charging helpers + new `ContractError` variants in `src/lib.rs`
  - [ ] Enforce charging on persistent writes in `mint_wrap` and `claim_wrap` (and any auxiliary counters)

- [ ] Add test for storage key collision between different data types
- [ ] Add `get_admin_pubkey` view function
  - [ ] Update `src/lib.rs`
  - [ ] Add/extend tests in `src/test.rs`
- [ ] Update `CONTRIBUTING.md` with development setup instructions
- [ ] Run `cargo fmt` / `cargo clippy` / `cargo test`

