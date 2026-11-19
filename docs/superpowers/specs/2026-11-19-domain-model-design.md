# Domain model design

Issue: #10. Related: #62 (internationalization), #11 (SQLite adapter).

## Intent

Give every later crate the same vocabulary: accounts, mails, tags,
classifications and moves, plus the rule that turns a classification into
a move. The types are pure, validated when built, and carry the Mailbot
invariants (no permanent deletion, the code decides every action).

## Goals

- Pure types in `server/crates/domain`, no I/O.
- Port Mailbot's rules that are pure logic: tag key derivation, spam
  confidence clamping, the `decide` rules, the junk rescue rule.
- Unit tests that pin Mailbot's edge cases.

## Non-goals

- Persistence (#11), provider adapters, the classifier prompt, the HTTP
  DTOs.
- Display-only flags: hidden from stats, snoozed, handled, follow-up
  dismissed. They only change where a mail shows up, so they live in
  SQLite and the read-side query services.
- Dry-run: it belongs to the use case that applies the decision.

## Anti-goals

- A `Delete` move, or any path to permanent deletion.
- User-facing text in the domain (see #62).
- `serde` derives on domain types: the HTTP layer converts to its own DTOs.

## Constraints

- `domain` depends on no I/O crate (checked by `app/tests/layering.rs`).
- Internationalization (#62): values are stable English codes, errors are
  codes with structured fields, never sentences. Mailbot's French values
  map to English: `haute/normale/basse` to `High/Normal/Low`, `inconnu`
  to `Unknown`.

## Approaches

1. **Validated newtypes and enums in `domain`, adapters convert**
   (chosen). Invalid values cannot exist past construction; every rule
   is testable without I/O.
2. Plain structs with public string fields, validation in use cases.
   Rejected: validation would be repeated and Mailbot's bugs (unchecked
   priority and category) would come back.
3. Shared types with `serde` used by every layer. Rejected: breaks the
   rule that the API has its own DTOs.

## Design

Modules of the `domain` crate:

- `account`: `AccountId`, `ProviderKind` (`Gmail`, `Outlook`, `Imap`),
  `EmailAddress` (validated, lowercased). `domain()` gives the part after
  `@`.
- `mail`: `ProviderMailId`, `ThreadId`, `Location` (`Inbox`, `Archive`,
  `Junk`, `Trash`, `Sent`, `Folder(FolderPath)`), `FolderPath` (levels
  separated by `/`, no empty level), `MailMeta` (account, provider id,
  thread, received at, sender address (`None` when the From header holds
  no address that parses, common in spam) and name, recipients (those
  that parse), subject,
  `dmarc_passed`, `is_read: Option<bool>`, `locked`, location).
- `tag`:
  - `TagKey::derive(name, taken)`: NFKD, drop non-ASCII, lowercase, runs
    of `[^a-z0-9]` become `-`, trim `-`, at most 30 characters, empty
    becomes `tag`, collision appends `-2`, `-3`…; `unknown` is reserved.
    A key never changes after creation.
  - `TagName` (trimmed, 1 to 40 characters; uniqueness compares
    lowercased names, accents significant), `TagDescription` (1 to 500), `Color`
    (`#rrggbb`, stored lowercase).
  - `Tag { key, name, description, color }`; `spam` is protected: it can
    be renamed, not deleted (`Tag::can_delete`).
  - Default tags: keys `important`, `personal`, `work`, `transaction`,
    `notification`, `newsletter`, `promotion`, `spam`. Their display name
    is translated by the client until the user renames them.
- `classification`: `Category` (`Tag(TagKey)` or `Unknown`), `Priority`
  (`High`, `Normal`, `Low`), `SpamConfidence` (0 to 100, `clamped` from
  any integer), `Classification { category, priority, needs_reply,
  is_phishing, spam_confidence, summary, reason }`. Token usage and model
  stay out of the domain (classifier port concern).
- `moves`: `Move` (`Inbox`, `Archive`, `Junk`, `Trash`,
  `Folder(FolderPath)`), no `Delete` variant.
- `decision`: `SpamPolicy { threshold: SpamConfidence, archive:
  BTreeSet<TagKey> }` and a pure `decide(input) -> Option<Move>` where the
  input carries the classification, the mail location, and booleans the
  caller computes through ports: `trusted`, `known_sender`, `locked`;
  `protect_known_senders` is part of the policy. Rules, in order:
  1. Locked: no move.
  2. In junk: `Inbox` (rescue) if trusted and DMARC passed, otherwise no
     move.
  3. Anywhere but the inbox: no move (as Mailbot, which only moves inbox
     mails).
  4. Trusted and DMARC passed: no move. A trusted sender failing DMARC
     is evaluated like any other (#24: the From may be spoofed).
  5. Spam category, or phishing, with confidence at or above the
     threshold: `Junk`, unless `protect_known_senders` and the sender is
     known with DMARC passed.
  6. Category in the archive set: `Archive`.
  7. Otherwise no move.
  A refusal (no classification) is handled by the caller: no move.
- Muted threads: `muted_thread_move(location, locked, thread_muted)`
  archives a new inbox mail of a muted thread unless it is locked. It runs
  before classification, so it needs no Claude call and applies to mails
  Claude refused; as in Mailbot, it is the user's choice and applies even
  in dry-run mode.
- Errors: one `thiserror` enum per module, variants are codes with fields
  (`TagNameTooLong { max }`), no message meant for the user.

Dependencies: `chrono` (no default features beyond what dates need),
`unicode-normalization`, `thiserror`. All pure.

## Left to the builder

- Exact constructor signatures and module file layout.
- Whether `known_sender` and `trusted` become one enum instead of two
  booleans, if it reads better.
- Test case list, as long as each Mailbot rule above has one.
