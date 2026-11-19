# Unified Mailbox

Sorts Gmail, Outlook and Proton Mail mailboxes with Claude: category,
priority, spam and phishing detection, rules, one-line summaries. A rewrite
of Mailbot as a headless Rust server with an API, and clients that only talk
to that API: a web app, and iOS and Android apps built from the same code.

Each provider is an adapter behind the same `MailProvider` port, so other
providers can be added later without touching the sorting logic: any IMAP
mailbox (Fastmail, iCloud, a self-hosted server…) or another provider's API.
Proton Mail has no public API: it is reached through
[Proton Mail Bridge](https://proton.me/mail/bridge), which exposes the
mailbox over IMAP on the server.

Status: foundations in progress, nothing usable yet. The target
architecture and the working rules are in [CLAUDE.md](CLAUDE.md).

## Prerequisites

- [mise](https://mise.jdx.dev), which installs the pinned tools
  (`mise.toml`).
- [Tailscale](https://tailscale.com) on the server and on every client
  device: the server is reachable only on the tailnet.
- For a Proton Mail account: Proton Mail Bridge running on the server,
  which requires a paid Proton plan.

## Installation

```bash
git clone git@github.com:cyboooooorg/unified-mailbox.git
cd unified-mailbox
mise install
```

## Usage and configuration

To be written with the first server code.

## Development

The server is a Cargo workspace in `server/`. `mise install` installs the
Rust toolchain pinned in `mise.toml`, with `clippy` and `rustfmt`; the
workspace declares the same version as its `rust-version`. Raise both
together.

Run the checks from `server/`, as the CI does on every pull request:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The hexagonal layering is enforced by the crate graph and checked by
`crates/app/tests/layering.rs`: `domain`, `ports` and `application` must
not reach an I/O crate (`tokio`, `sqlx`, `reqwest`, `axum`…), even
transitively, and may depend only on the inner workspace crates. When an
adapter brings a new I/O crate, add it to `IO_CRATES` in that test.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this project by you, as defined in the
Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
