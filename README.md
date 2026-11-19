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

Status: design stage, no code yet. The target architecture and the working
rules are in [CLAUDE.md](CLAUDE.md).

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
