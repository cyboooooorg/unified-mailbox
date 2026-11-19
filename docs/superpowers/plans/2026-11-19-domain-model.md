# Domain Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pure, validated domain types for accounts, mails, tags,
classifications and moves, plus the pure `decide` rule.

**Architecture:** One module per concept in `server/crates/domain/src/`,
re-exported from `lib.rs`. Constructors validate and return a per-module
`thiserror` error whose variants are codes with fields. No I/O, no
`serde`.

**Tech Stack:** Rust 1.99 (edition 2024), `chrono` (no default features,
`std` only), `unicode-normalization`, `thiserror`.

**Spec:** `docs/superpowers/specs/2026-11-19-domain-model-design.md`

## Global Constraints

- `domain` must pass `cargo test -p app --test layering`: no I/O crate,
  no workspace crate dependency.
- No `Delete` variant in `Move`, no path to permanent deletion.
- No user-facing text: error variants are codes with structured fields;
  no `#[error("…")]` sentence aimed at users (a short developer
  `Display` such as `"tag_name_too_long"` is fine).
- No `serde` in `domain`.
- English codes only: `High/Normal/Low`, `Unknown` (not `haute`,
  `inconnu`).
- Checks from `server/`: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.
- Commits: Conventional Commits, scope `domain`, footer `Refs: #10`.

## Review Focus

- A tag name whose characters are all non-ASCII (`"日本"`, `"🎉"`): key
  must become `tag`, then `tag-2` on collision. Pinned in Task 3.
- A name that slugs to the reserved key (`"Unknown"`, `"UNKNOWN!"`):
  must get `unknown-2`, never `unknown`. Pinned in Task 3.
- Truncation at 30 characters leaving a trailing `-`, and the collision
  suffix pushing past 30: key stays at most 30 characters with no
  trailing `-`. Pinned in Task 3.
- Spam confidence out of range from the model (`-5`, `250`): clamped to
  0 and 100. Pinned in Task 4.
- A locked mail in junk from a trusted, authenticated sender: no move
  (lock wins over rescue). Pinned in Task 5.

---

### Task 1: Crate dependencies and `account`

**Files:**
- Modify: `server/Cargo.toml` (`[workspace.dependencies]`)
- Modify: `server/crates/domain/Cargo.toml`
- Modify: `server/crates/domain/src/lib.rs`
- Create: `server/crates/domain/src/account.rs` (tests in a
  `#[cfg(test)] mod tests` at the bottom, same for every module)

**Interfaces:**
- Produces:
  - `pub struct AccountId(i64)` with `new(i64) -> Self`, `get(self) -> i64`;
    derives `Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord`.
  - `pub enum ProviderKind { Gmail, Outlook, Imap }` with
    `as_code(self) -> &'static str` (`"gmail"`, `"outlook"`, `"imap"`)
    and `from_code(&str) -> Option<Self>`.
  - `pub struct EmailAddress(String)` with
    `parse(&str) -> Result<Self, AccountError>`, `as_str(&self) -> &str`,
    `domain(&self) -> &str`.
  - `pub enum AccountError { InvalidEmail }`.

- [ ] **Step 1: Add dependencies.** Workspace: `chrono = { version = "0.4",
  default-features = false, features = ["std"] }`,
  `unicode-normalization = "0.1"`, `thiserror = "2"`. `domain` uses all
  three with `.workspace = true`.
- [ ] **Step 2: Write the failing tests**

```rust
#[test] fn email_is_trimmed_and_lowercased() {
    assert_eq!(EmailAddress::parse("  Alice@Example.COM ").unwrap().as_str(), "alice@example.com");
}
#[test] fn email_domain() {
    assert_eq!(EmailAddress::parse("a@mail.example.org").unwrap().domain(), "mail.example.org");
}
#[test] fn email_rejects_invalid() {
    for bad in ["", "alice", "@example.com", "alice@", "a@b@c.com", "a b@example.com"] {
        assert_eq!(EmailAddress::parse(bad), Err(AccountError::InvalidEmail), "{bad}");
    }
}
#[test] fn provider_codes_round_trip() {
    for p in [ProviderKind::Gmail, ProviderKind::Outlook, ProviderKind::Imap] {
        assert_eq!(ProviderKind::from_code(p.as_code()), Some(p));
    }
    assert_eq!(ProviderKind::from_code("yahoo"), None);
}
```

- [ ] **Step 3: Run** `cargo test -p domain account` — expected: compile
  failure (types not defined).
- [ ] **Step 4: Implement.** Validation is deliberately loose (exactly one
  `@`, non-empty local part and domain, no whitespace); providers are the
  authority on deliverability. `lib.rs`: `pub mod account;` and
  `pub use account::*;` (same for every later module).
- [ ] **Step 5: Run** `cargo test --workspace` (layering included) —
  expected: PASS.
- [ ] **Step 6: Commit** `feat(domain): add account types`.

### Task 2: `mail`

**Files:**
- Create: `server/crates/domain/src/mail.rs`

**Interfaces:**
- Consumes: `AccountId`, `EmailAddress` (Task 1).
- Produces:
  - `pub struct ProviderMailId(String)` and `pub struct ThreadId(String)`,
    each `new(impl Into<String>) -> Result<Self, MailError>` (rejects
    empty) and `as_str`.
  - `pub struct FolderPath(Vec<String>)` with
    `parse(&str) -> Result<Self, MailError>` (levels split on `/`, each
    trimmed, none empty), `levels(&self) -> &[String]`,
    `to_path_string(&self) -> String` (joined with `/`).
  - `pub enum Location { Inbox, Archive, Junk, Trash, Sent, Folder(FolderPath) }`.
  - `pub struct MailMeta { pub account: AccountId, pub provider_id:
    ProviderMailId, pub thread: Option<ThreadId>, pub received_at:
    chrono::DateTime<chrono::Utc>, pub sender: EmailAddress, pub
    sender_name: Option<String>, pub recipients: Vec<EmailAddress>, pub
    subject: String, pub dmarc_passed: bool, pub is_read: Option<bool>,
    pub locked: bool, pub location: Location }` (plain data, public
    fields: every field is already validated by its type).
  - `pub enum MailError { EmptyProviderId, EmptyThreadId, EmptyFolderPath,
    EmptyFolderLevel }`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn folder_path_trims_levels() {
    let p = FolderPath::parse(" Clients / Acme ").unwrap();
    assert_eq!(p.levels(), ["Clients", "Acme"]);
    assert_eq!(p.to_path_string(), "Clients/Acme");
}
#[test] fn folder_path_rejects_empty() {
    assert_eq!(FolderPath::parse(""), Err(MailError::EmptyFolderPath));
    assert_eq!(FolderPath::parse("a//b"), Err(MailError::EmptyFolderLevel));
    assert_eq!(FolderPath::parse("a/ "), Err(MailError::EmptyFolderLevel));
}
#[test] fn ids_reject_empty() {
    assert_eq!(ProviderMailId::new(""), Err(MailError::EmptyProviderId));
    assert_eq!(ThreadId::new(""), Err(MailError::EmptyThreadId));
}
```

- [ ] **Step 2: Run** `cargo test -p domain mail` — expected: compile failure.
- [ ] **Step 3: Implement** the types above.
- [ ] **Step 4: Run** `cargo test --workspace` — expected: PASS.
- [ ] **Step 5: Commit** `feat(domain): add mail metadata and locations`.

### Task 3: `tag`

**Files:**
- Create: `server/crates/domain/src/tag.rs`

**Interfaces:**
- Produces:
  - `pub struct TagKey(String)`:
    - `derive(name: &str, taken: &impl Fn(&str) -> bool) -> TagKey`
      (`taken` answers whether a key is already used);
    - `from_stored(&str) -> Result<Self, TagError>`, which accepts any
      string matching `^[a-z0-9]+(-[a-z0-9]+)*$` of at most 30
      characters (otherwise `TagError::InvalidKey`), and rejects
      `unknown` (`TagError::ReservedKey`);
    - `as_str`;
    - `pub const RESERVED: &str = "unknown"`, `pub const MAX_LEN: usize = 30`.
  - `pub struct TagName(String)`: `parse(&str) -> Result<Self, TagError>`
    (trimmed, 1..=40 chars), `as_str`,
    `same_as(&self, other: &TagName) -> bool` (compares
    `to_lowercase()`).
  - `pub struct TagDescription(String)`: `parse` (trimmed, 1..=500 chars).
  - `pub struct Color(String)`: `parse` (`#` and 6 hex digits, stored
    lowercase), `as_str`, and `Color::default_color() -> Color`
    returning `#868e96`.
  - `pub struct Tag { pub key: TagKey, pub name: TagName, pub description:
    TagDescription, pub color: Color }` with `can_delete(&self) -> bool`
    (false for key `spam`).
  - `pub const DEFAULT_TAG_KEYS: [&str; 8] = ["important", "personal",
    "work", "transaction", "notification", "newsletter", "promotion",
    "spam"]`.
  - `pub enum TagError { NameEmpty, NameTooLong { max: usize },
    DescriptionEmpty, DescriptionTooLong { max: usize }, InvalidColor,
    InvalidKey, ReservedKey }`.

- [ ] **Step 1: Write the failing tests**

```rust
fn none(_: &str) -> bool { false }
#[test] fn key_slugs_accents_and_punctuation() {
    assert_eq!(TagKey::derive("Équipe & Clients!", &none).as_str(), "equipe-clients");
}
#[test] fn key_falls_back_to_tag() {
    assert_eq!(TagKey::derive("日本", &none).as_str(), "tag");
    assert_eq!(TagKey::derive("🎉", &|k| k == "tag").as_str(), "tag-2");
}
#[test] fn key_appends_suffix_on_collision() {
    let taken = |k: &str| k == "work" || k == "work-2";
    assert_eq!(TagKey::derive("Work", &taken).as_str(), "work-3");
}
#[test] fn key_never_reserved() {
    assert_eq!(TagKey::derive("Unknown!", &none).as_str(), "unknown-2");
}
#[test] fn key_truncates_without_trailing_dash() {
    let k = TagKey::derive("abcdefghijklmnopqrstuvwxyz abcd efgh", &none);
    assert!(k.as_str().len() <= 30 && !k.as_str().ends_with('-'));
    let long = "a".repeat(40);
    let k2 = TagKey::derive(&long, &|k| k.len() == 30);
    assert!(k2.as_str().len() <= 30 && k2.as_str().ends_with("-2"));
}
#[test] fn stored_key_rejects_reserved_and_bad() {
    assert_eq!(TagKey::from_stored("unknown"), Err(TagError::ReservedKey));
    assert_eq!(TagKey::from_stored("Bad Key"), Err(TagError::InvalidKey));
}
#[test] fn name_rules() {
    assert_eq!(TagName::parse("  ").unwrap_err(), TagError::NameEmpty);
    assert_eq!(TagName::parse(&"x".repeat(41)).unwrap_err(), TagError::NameTooLong { max: 40 });
    assert!(TagName::parse("Travail").unwrap().same_as(&TagName::parse("TRAVAIL").unwrap()));
    assert!(!TagName::parse("Été").unwrap().same_as(&TagName::parse("Ete").unwrap()));
}
#[test] fn color_is_lowercased() {
    assert_eq!(Color::parse("#AABBCC").unwrap().as_str(), "#aabbcc");
    assert_eq!(Color::parse("aabbcc").unwrap_err(), TagError::InvalidColor);
}
#[test] fn spam_cannot_be_deleted() { /* build a Tag with key "spam" and one with "work"; assert can_delete false / true */ }
```

- [ ] **Step 2: Run** `cargo test -p domain tag` — expected: compile failure.
- [ ] **Step 3: Implement.** `derive`: NFKD (`unicode_normalization`),
  keep ASCII only, lowercase, replace runs of non-`[a-z0-9]` with `-`,
  trim `-`, truncate to 30 and trim `-` again, empty becomes `tag`. Then
  if the base is `unknown` or taken, try `base-2`, `base-3`…, truncating
  `base` so that `base-n` stays within 30 characters (trim `-` after
  truncating). Lengths count characters; after slugging, all are ASCII.
- [ ] **Step 4: Run** `cargo test --workspace` — expected: PASS.
- [ ] **Step 5: Commit** `feat(domain): add tags with stable keys`.

### Task 4: `classification`

**Files:**
- Create: `server/crates/domain/src/classification.rs`

**Interfaces:**
- Consumes: `TagKey` (Task 3).
- Produces:
  - `pub enum Category { Tag(TagKey), Unknown }` with
    `is_tag(&self, key: &str) -> bool`.
  - `pub enum Priority { High, Normal, Low }` with `as_code`
    (`"high"`, `"normal"`, `"low"`) and `from_code`.
  - `pub struct SpamConfidence(u8)`: `clamped(i64) -> Self` (0..=100),
    `new(u8) -> Option<Self>` (`None` above 100), `get(self) -> u8`;
    derives `PartialOrd, Ord`.
  - `pub struct Classification { pub category: Category, pub priority:
    Priority, pub needs_reply: bool, pub is_phishing: bool, pub
    spam_confidence: SpamConfidence, pub summary: String, pub reason:
    String }`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn confidence_is_clamped() {
    assert_eq!(SpamConfidence::clamped(-5).get(), 0);
    assert_eq!(SpamConfidence::clamped(250).get(), 100);
    assert_eq!(SpamConfidence::clamped(42).get(), 42);
    assert_eq!(SpamConfidence::new(101), None);
}
#[test] fn priority_codes_round_trip() {
    for p in [Priority::High, Priority::Normal, Priority::Low] {
        assert_eq!(Priority::from_code(p.as_code()), Some(p));
    }
    assert_eq!(Priority::from_code("haute"), None);
}
#[test] fn category_matches_tag_key() {
    let c = Category::Tag(TagKey::from_stored("spam").unwrap());
    assert!(c.is_tag("spam") && !Category::Unknown.is_tag("spam"));
}
```

- [ ] **Step 2: Run** `cargo test -p domain classification` — expected: compile failure.
- [ ] **Step 3: Implement** the types above.
- [ ] **Step 4: Run** `cargo test --workspace` — expected: PASS.
- [ ] **Step 5: Commit** `feat(domain): add classification types`.

### Task 5: `Move`, `SpamPolicy` and `decide`, then docs

**Files:**
- Create: `server/crates/domain/src/moves.rs`
- Create: `server/crates/domain/src/decision.rs`
- Modify: `server/crates/domain/src/lib.rs` (crate docs)

**Interfaces:**
- Consumes: `FolderPath`, `Location` (Task 2), `TagKey` (Task 3),
  `Classification`, `Category`, `SpamConfidence` (Task 4).
- Produces:
  - `pub enum Move { Inbox, Archive, Junk, Trash, Folder(FolderPath) }`,
    doc comment stating there is deliberately no `Delete`.
  - `pub struct SpamPolicy { pub threshold: SpamConfidence, pub archive:
    BTreeSet<TagKey>, pub protect_known_senders: bool }`.
  - `pub struct DecisionInput<'a> { pub classification: &'a
    Classification, pub location: &'a Location, pub dmarc_passed: bool,
    pub locked: bool, pub trusted: bool, pub known_sender: bool, pub
    thread_muted: bool }`.
  - `pub fn decide(input: &DecisionInput<'_>, policy: &SpamPolicy) -> Option<Move>`.

- [ ] **Step 1: Write the failing tests.** A helper `input()` builds an
  inbox mail, category `work`, priority Normal, not phishing, confidence
  0, every flag false; a `policy()` has threshold 90, empty archive set,
  protection on. One test per rule:

```rust
#[test] fn locked_never_moves() { /* spam 100 in inbox, locked → None */ }
#[test] fn locked_in_junk_is_not_rescued() { /* Junk, trusted, dmarc → None */ }
#[test] fn junk_rescued_when_trusted_and_authenticated() { /* → Some(Move::Inbox) */ }
#[test] fn junk_stays_without_dmarc() { /* trusted, dmarc false → None */ }
#[test] fn junk_stays_when_not_trusted() { /* dmarc true → None */ }
#[test] fn muted_thread_in_inbox_is_archived() { /* → Some(Move::Archive) */ }
#[test] fn trusted_sender_is_not_moved() { /* spam 100 → None */ }
#[test] fn spam_at_threshold_goes_to_junk() { /* spam 90 → Some(Move::Junk); spam 89 → None */ }
#[test] fn phishing_needs_the_threshold() { /* phishing, category work, 50 → None; 95 → Junk */ }
#[test] fn known_authenticated_sender_is_spared() { /* spam 100, known, dmarc → None */ }
#[test] fn known_sender_without_dmarc_is_not_spared() { /* → Junk */ }
#[test] fn protection_off_lets_threshold_decide() { /* known, dmarc, protect false → Junk */ }
#[test] fn archive_category_is_archived() { /* archive = {newsletter}, category newsletter → Archive */ }
#[test] fn unknown_category_is_not_moved() { /* Category::Unknown, 0 → None */ }
```

- [ ] **Step 2: Run** `cargo test -p domain decision` — expected: compile failure.
- [ ] **Step 3: Implement `decide`** with the rules in this order:
  locked → `None`; location `Junk` → `Inbox` if `trusted && dmarc_passed`
  else `None`; `thread_muted && location == Inbox` → `Archive`;
  `trusted` → `None`; (`category` is tag `spam` or `is_phishing`) and
  `confidence >= threshold` → `None` if `protect_known_senders &&
  known_sender && dmarc_passed`, else `Junk`; category in `archive` →
  `Archive`; otherwise `None`.
- [ ] **Step 4: Update the crate docs** in `lib.rs`: one paragraph per
  module saying what it holds, that values are English codes translated
  by the client (#62), and that `Move` has no `Delete` by design.
- [ ] **Step 5: Run** all checks from `server/`:
  `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  — expected: all PASS.
- [ ] **Step 6: Commit** `feat(domain): add moves and the decide rule`.
