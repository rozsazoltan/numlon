# Contributing

Keep changes small, focused, and easy to review.

Use Conventional Commits for commit messages.

Run before opening a pull request:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
