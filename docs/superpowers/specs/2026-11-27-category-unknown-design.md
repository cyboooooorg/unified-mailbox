# What `Category::Unknown` means

Issue: #63. Related: #10 (domain model), #11 (sqlite adapter), #12
(configuration), #19 (classifier port and Claude API adapter), #22
(decision engine), #25 (tags), #18 (sync loop).

## Intent

The domain docs contradict each other: `classification.rs` says
`Category::Unknown` is what the classifier returns when it refuses,
`decision.rs` says a refusal has no `Classification` at all. Because
`decide` reads `is_phishing` whatever the category, an adapter that
built `Classification { category: Unknown, is_phishing: true, .. }`
would send a mail Claude refused to junk. This design settles the
meaning so that case cannot be built.

## Mailbot's behavior (reference)

- The model cannot produce `inconnu`: the JSON schema restricts
  `category` to the live tag keys (`app/classifier.py:50-64`).
- `inconnu` is a stored state written by the worker when Claude was
  asked about a recent mail and returned no result: a refusal, or a
  truncated answer (`max_tokens`) treated the same way
  (`app/classifier.py:236-254`, `app/worker.py:572-578`).
- A Claude error is not `inconnu`: the mail is stored with a `NULL`
  category, "indexed without AI" (`tests/test_claude_resilience.py`).
- A refused mail gets no label and no move, whatever it looks like
  (`app/worker.py:711-712`).
- `inconnu` is not a tag. It is a reserved key (`app/main.py:1316`),
  rejected for a manual category change, a rule target or condition,
  and the archive categories (`app/main.py:237-250`,
  `app/config.py:249-254`).
- The 90-day check does not send a refused mail to Claude again (no
  new bill), but a Mailbot label found in the mailbox replaces
  `inconnu` (`app/worker.py:1013-1021`,
  `tests/test_audit_worker.py:227-243`).
- A refused mail does not count towards making its sender known
  (`app/db.py:2268-2272`).

## Decisions

### `Unknown` is a stored state, never a classifier output

- `Classification.category` becomes a `TagKey`. A classifier result
  always names a tag; a refusal is not a `Classification` (the port of
  #19 returns it as a distinct outcome).
- `Category { Tag(TagKey), Unknown }` stays, as the category stored for
  a mail. `Unknown` means "the classifier refused this mail". The name
  is kept: it matches the reserved key `unknown`, the stored code
  (`mails.category = 'unknown'`, migration `0001_initial.sql`) and
  Mailbot's meaning ("Non classé" in its interface).
- `decide` no longer sees `Unknown`: the `Category::Unknown => false`
  arm and its test go away. A refused mail is not moved, as in Mailbot;
  only the muted-thread move (`muted_thread_move`) applies to it.

### Kept from Mailbot

All the behaviors listed above, unchanged. The ones that need storage
or the sync loop (90-day check skip, label adoption, known-sender
count, reserved key in rules and manual changes) are implemented by
their own issues (#18, #22, #24, #25, #43, #44); this design only fixes
what they rely on.

### Changed from Mailbot

- `ARCHIVE_CATEGORIES=unknown` gets its own error message (a reserved
  key, not a malformed one) instead of the generic "a tag key"
  expectation (`server/crates/app/src/config.rs`, `tag_keys`).

### Left to #19

- The adapter checks that the returned category is a live tag key. A
  key outside the list is an error for that mail, not an orphan
  category stored and labelled as Mailbot would.
- A truncated answer is told apart from a refusal, at least in the
  stored reason: Mailbot's "the model refused" is wrong for it.

### Dropped

Nothing.

## Scope of the change

- `server/crates/domain/src/classification.rs`: `category: TagKey`,
  doc comments of `Category` and `Classification`.
- `server/crates/domain/src/decision.rs`: archive check on the
  `TagKey`, doc of `decide`, test fixtures.
- `server/crates/domain/src/tag.rs`: doc of `TagKey::RESERVED`.
- `server/crates/app/src/config.rs`: reserved-key message, with a test
  for `ARCHIVE_CATEGORIES=unknown`.
- `docs/superpowers/specs/2026-11-19-domain-model-design.md`: the lines
  on `Category` and refusals.

No migration, no API or client change: nothing outside `domain` and
`app` reads or writes a category yet.

## Non-goals

- The `Classifier` port and its refusal outcome (#19).
- Re-filing the mails of a deleted tag (#25).
