# `Category::Unknown` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `Category::Unknown` a stored state only ("the classifier
refused"), so a `Classification` always names a tag, and give
`ARCHIVE_CATEGORIES=unknown` its own error message.

**Architecture:** `Classification.category` becomes a `TagKey`;
`Category { Tag, Unknown }` stays for the category stored with a mail.
`decide` loses its `Unknown` arm. The configuration parser tells a
reserved key apart from a malformed one through `TagError::ReservedKey`.

**Tech Stack:** Rust (workspace in `server/`), `mise run ci`.

**Spec:** `docs/superpowers/specs/2026-11-27-category-unknown-design.md`

## Global Constraints

- `domain` depends on no I/O crate.
- No user-facing sentence in `domain`; configuration errors are for the
  operator and stay in English.
- Branch `fix/category-unknown`, Conventional Commits, scope `domain` or
  `app`.
- `mise run ci` passes before the PR.

## Review Focus

- `ARCHIVE_CATEGORIES=Unknown` (any case): lower-cased first, so it must
  get the reserved message too, not be accepted. Pinned in Task 2.
- `ARCHIVE_CATEGORIES=newsletter,unknown,a b`: one error per bad item,
  each with its own message. Pinned in Task 2.
- Phishing with a high confidence still goes to junk whatever tag it
  carries (the `is_phishing` path no longer depends on `Category`).
  Already pinned by `phishing_needs_the_threshold`; keep it passing.

---

### Task 1: `Classification` names a tag

**Files:**

- Modify: `server/crates/domain/src/classification.rs` (doc of
  `Category`, `Classification.category`)
- Modify: `server/crates/domain/src/decision.rs` (`decide`, its doc,
  test fixtures, remove `unknown_category_is_not_moved`)
- Modify: `server/crates/domain/src/tag.rs:31` (doc of `RESERVED`)
- Modify: `docs/superpowers/specs/2026-11-19-domain-model-design.md`
  (the `classification` bullet and the "A refusal" line)

**Interfaces:**

- Produces: `Classification { category: TagKey, .. }` (other fields
  unchanged). `Category { Tag(TagKey), Unknown }` unchanged in shape;
  its doc says it is the category stored for a mail, `Unknown` meaning
  the classifier refused that mail. `decide` signature unchanged.

No new failing test in this task: the change is to a type, and the
compiler lists every use to fix. The deleted test is the case the new
type makes impossible to build.

- [ ] **Step 1: Change the field and let the compiler list the uses**

`pub category: TagKey` in `Classification`. Doc of `Category`: "The
category stored for a mail: the tag of its classification, or `Unknown`
when the classifier refused to classify it. A `Classification` always
names a tag." Doc of `TagKey::RESERVED`: "Reserved for
`Category::Unknown` (the classifier refused the mail); never a tag."

- [ ] **Step 2: Update `decide`**

Drop `Category` from the `use crate::classification::…` line at the
top of `decision.rs` (it becomes unused and `clippy -D warnings` fails
on it); write the doc link below as
``[`Category::Unknown`](crate::classification::Category::Unknown)``.
Suspicious check: `classification.category.as_str() == SPAM_KEY ||
classification.is_phishing`. Archive check:
`policy.archive.contains(&classification.category)`. Doc: replace "A
mail the classifier refused has no classification: the caller does not
move it." with "A mail the classifier refused has no `Classification`
(its stored category is `Category::Unknown`): the caller does not move
it."

- [ ] **Step 3: Update the tests**

Fixtures use `key("…")` instead of `Category::Tag(key("…"))` (in
`fixture`, `Fixture::spam`, `mails_outside_the_inbox_are_not_moved`,
`archive_category_is_archived`), and `Category` leaves the test
module's `use` line. Delete
`unknown_category_is_not_moved`: the case can no longer be built. Keep
`category_matches_tag_key` in `classification.rs` (it pins
`Category::is_tag` on the stored type).

- [ ] **Step 4: Run the domain tests**

Run: `cargo test -p domain` (from `server/`)
Expected: PASS, every test of `decision` still green.

- [ ] **Step 5: Update the domain-model spec**

`classification` bullet: "`Category` (`Tag(TagKey)` or `Unknown`, the
category stored for a mail; `Unknown` means the classifier refused it,
see `2026-11-27-category-unknown-design.md`) … `Classification {
category: TagKey, priority, … }`". Rule list: "A refusal has no
`Classification` (stored as `Category::Unknown`): the caller does not
move the mail."

- [ ] **Step 6: Commit**

```bash
git add server/crates/domain docs/superpowers/specs docs/superpowers/plans
git commit -m "fix(domain): make a classification always name a tag"
```

Body: `Category::Unknown` is the stored state of a refused mail, never a
classifier output, so `decide` cannot send a refused mail to junk.
Footer: `Refs: #63`.

### Task 2: reserved key in `ARCHIVE_CATEGORIES`

**Files:**

- Modify: `server/crates/app/src/config.rs:197-223` (`tag_keys`) and
  its tests
- Modify: `README.md:105` (Configuration table)

**Interfaces:**

- Consumes: `TagKey::from_stored(&str) -> Result<TagKey, TagError>`,
  `TagError::ReservedKey`, `TagKey::RESERVED` (`domain/src/tag.rs`).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn archive_categories_reserved_key_has_its_own_message() {
    for value in ["unknown", "Unknown"] {
        let errs = errors(&[("ARCHIVE_CATEGORIES", value)]);
        assert_eq!(errs.len(), 1, "{value}");
        assert!(errs[0].to_string().contains("reserved"), "{}", errs[0]);
    }
    let errs = errors(&[("ARCHIVE_CATEGORIES", "newsletter,unknown,a b")]);
    assert_eq!(errs.len(), 2);
    assert!(errs[0].to_string().contains("reserved"), "{}", errs[0]);
    assert!(!errs[1].to_string().contains("reserved"), "{}", errs[1]);
}
```

`errs` keeps the items in the order they appear in the value, as
`tag_keys` already does (its `invalid` list is filled in order).

- [ ] **Step 2: Run it to make sure it fails**

Run: `cargo test -p app archive_categories_reserved` (from `server/`)
Expected: FAIL, the message has no "reserved".

- [ ] **Step 3: Implement**

Add `TagError` to the `use domain::{…}` line of `config.rs`. In
`tag_keys`, keep the expectation with each invalid item in `invalid`,
match `Err(TagError::ReservedKey)` and fail with the
expectation `"a tag key: 'unknown' is reserved for mails the classifier
refused"` (build it from `TagKey::RESERVED`); other errors keep the
current expectation.

- [ ] **Step 4: Run the app tests**

Run: `cargo test -p app`
Expected: PASS, including `archive_categories_each_bad_item_reported`.

- [ ] **Step 5: Document**

README line 105: append "`unknown` is reserved (mails the classifier
refused) and rejected."

- [ ] **Step 6: Run the whole pipeline and commit**

Run: `mise run ci`
Expected: every task passes.

```bash
git add server/crates/app/src/config.rs README.md
git commit -m "fix(app): reject the reserved unknown key with its own message"
```

Footer: `Closes: #63` (this commit completes the issue).
