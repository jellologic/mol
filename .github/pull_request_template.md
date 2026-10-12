## Tracker

<!-- Required: phase, sub-phase, or criterion id, e.g. P1.1 (commit-msg hook enforces) -->
Ref: <!-- P?.? -->

## What / proof

<!-- What changed; which proof suite is green -->

## New dependencies (doc 23 checklist — delete section if none)

- [ ] No Mac-observable quirk delegated without a differential test
- [ ] `default-features = false` + explicit features
- [ ] Transitive tree measured (`cargo tree`) and recorded
- [ ] MSRV/edition checked against `rust-toolchain.toml`
- [ ] License on the `cargo deny` allow-list
- [ ] Behind a trait seam if upstream could drift from Mac behavior
- [ ] `Cargo.lock` + `vendor/` updated in the same change
