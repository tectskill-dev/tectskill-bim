## Summary

<!-- What does this change do, and why? -->

Fixes #

## Screenshots

<!-- For visual changes, before and after. Delete if not applicable. -->

## Checklist

- [ ] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo nextest run --workspace` pass.
- [ ] New interface text is in `assets/i18n/*.ftl` for every language.
- [ ] Model changes go through `Document::edit`, and older `.ccad` files still load.
- [ ] Commits are signed off (`git commit -s`) under the DCO.
- [ ] This contribution follows the clean-room policy in CONTRIBUTING.md.
