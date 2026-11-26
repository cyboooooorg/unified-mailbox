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

Run the server from `server/`:

```bash
cargo run -p app
```

The server serves the API on `LISTEN_ADDR` until it receives SIGTERM or
Ctrl-C, then stops cleanly. Every route is under `/api/v1`; the contract
is `api/openapi.json`. See [Pairing a device](#pairing-a-device) to get
a token.

### Configuration

The server reads its configuration once, at startup, from environment
variables. A `.env` file in the working directory is read too, if there
is one, and a variable set in the real environment wins over the same
one in `.env`. Copy `.env.example` into the directory you run the server
from (`server/.env` with the command above) to start.

- Names are exact and upper case: `dry_run` is not `DRY_RUN`, and is
  ignored.
- An unset variable, or one set to an empty value, takes its default.
  Values are trimmed.
- The server checks every variable at startup. If any is invalid, it
  prints all of them, one per line, and exits with status 1.
- Booleans accept `true`/`false`, `t`/`f`, `1`/`0`, `yes`/`no`, `y`/`n`
  and `on`/`off`, in any case.
- In `.env`, `$NAME` and `${NAME}` are replaced by the variable's value,
  unquoted or in double quotes. Put a value holding a `$` in single
  quotes to keep it as written.
- A malformed `.env` stops the server with the line number only, never
  the line's content.

| Variable | Default | Meaning |
| --- | --- | --- |
| `DATA_DIR` | `./data` | Directory of the database (`unified-mailbox.db`) and, by default, of the key file. Created with mode `0700` when missing. |
| `KEY_FILE` | `$DATA_DIR/secret.key` | The secret key file (see below). Its directory must exist. |
| `DRY_RUN` | `true` | Change nothing in the mailboxes, except actions the user confirmed. |
| `SPAM_THRESHOLD` | `90` | Spam or phishing confidence, from 0 to 100, at or above which a mail goes to the junk folder. |
| `PROTECT_KNOWN_SENDERS` | `true` | Never send to junk a mail from a sender the user knows and that passed DMARC. |
| `ARCHIVE_CATEGORIES` | empty | Comma-separated tag keys (such as `newsletter,promotion`) whose mails are archived once classified. |
| `LISTEN_ADDR` | `127.0.0.1:8000` | Socket address (`host:port`, IPv6 as `[::1]:8000`) the API listens on. Behind Tailscale, keep the loopback address and expose it with `tailscale serve`, or use the tailnet address. In Docker, use `0.0.0.0:8000` and publish the port on the tailnet interface only. |
| `RUST_LOG` | `info` | Log filter, in [`tracing-subscriber` `EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) syntax, such as `info,sqlite=debug`. |

Each later feature adds its variables to this table and to
`.env.example`.

### Pairing a device

Every route but `GET /api/v1/health` and `POST /api/v1/pairing` needs an
`Authorization: Bearer <token>` header. A token belongs to one device and
is revocable on its own.

For the first device, print a one-time code on the server (from
`server/`, or `unified-mailbox pair-code` in the container):

```bash
cargo run -p app -- pair-code
```

It prints a code such as `04HM-ASW9`, valid for 10 minutes and usable
once. Exchange it for a token, giving the device a name:

```bash
curl -X POST http://127.0.0.1:8000/api/v1/pairing \
  -H 'Content-Type: application/json' \
  -d '{"code": "04HM-ASW9", "name": "My phone"}'
```

The response holds the token (`um_...`). It is shown once and stored only
as a hash: keep it. Send it as `Authorization: Bearer <token>`.

- A paired device creates the code of the next one with `POST
  /api/v1/devices/pairing-codes`.
- `GET /api/v1/devices` lists the devices, and `DELETE
  /api/v1/devices/{id}` revokes one: its token stops working at once.
- When every device is lost or revoked, run `pair-code` again on the
  server.

### Data files

The server keeps its state in one SQLite database (with its `-wal` and
`-shm` files beside it), created with mode `0600` at the first start, in
`DATA_DIR`.

### Secret key

The OAuth tokens and IMAP passwords of the connected accounts are
encrypted in the database with a 32-byte key kept in its own file,
`KEY_FILE`. Set it outside `DATA_DIR` to keep the key on another disk or
in another backup.

- The server creates the file at its first start, with mode `0600`, and
  logs that it did. It refuses to start if the file exists with another
  length, or readable by group or others.
- Back the key file up apart from the database: a copy of the database
  alone does not expose the secrets, and the two together do.
- Losing the key does not lose the mail index, but every account must be
  connected again.

## Development

The server is a Cargo workspace in `server/`. `mise install` installs the
Rust toolchain pinned in `mise.toml`, with `clippy` and `rustfmt`; the
workspace declares the same version as its `rust-version`. Raise both
together.

Run the checks as the CI does on every pull request (format, clippy and
tests, defined in `mise.toml`):

```bash
mise run ci
```

The hexagonal layering is enforced by the crate graph and checked by
`crates/app/tests/layering.rs`: `domain`, `ports` and `application` must
not reach an I/O crate (`tokio`, `sqlx`, `reqwest`, `axum`…), even
transitively, and may depend only on the inner workspace crates. When an
adapter brings a new I/O crate, add it to `IO_CRATES` in that test.

The API contract is `api/openapi.json`, generated from the `http-api`
crate. After any change to a route or a DTO, regenerate it in the same
commit. `mise run openapi` regenerates it, and `mise run ci` fails while
it is stale:

```bash
mise run openapi
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this project by you, as defined in the
Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
