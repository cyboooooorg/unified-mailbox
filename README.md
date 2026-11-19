# Unified Mailbox

Sorts Gmail and Outlook mailboxes with Claude: category, priority, spam and
phishing detection, rules, one-line summaries. A rewrite of Mailbot as a
headless Rust server with an API, and clients that only talk to that API:
a web app, and iOS and Android apps built from the same code.

Status: design stage, no code yet. The target architecture and the working
rules are in [CLAUDE.md](CLAUDE.md).

## Prerequisites

- [mise](https://mise.jdx.dev), which installs the pinned tools
  (`mise.toml`).
- [Tailscale](https://tailscale.com) on the server and on every client
  device: the server is reachable only on the tailnet.

## Installation

```bash
git clone git@github.com:cyboooooorg/unified-mailbox.git
cd unified-mailbox
mise install
```

## Usage and configuration

To be written with the first server code.
