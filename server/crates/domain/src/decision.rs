//! The rule that turns a classification into a move.

use std::collections::BTreeSet;

use crate::classification::{Category, Classification, SpamConfidence};
use crate::mail::Location;
use crate::moves::Move;
use crate::tag::TagKey;

/// Key of the tag that marks spam.
const SPAM_KEY: &str = "spam";

/// The user's spam and archive settings, read once from the
/// configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpamPolicy {
    /// Spam or phishing at or above this confidence goes to junk.
    pub threshold: SpamConfidence,
    /// Categories archived as soon as they are classified.
    pub archive: BTreeSet<TagKey>,
    /// Spare senders the user already knows (see `DecisionInput::known_sender`).
    pub protect_known_senders: bool,
}

/// Everything `decide` needs about one mail. The caller computes the
/// flags that need storage through ports.
#[derive(Clone, Copy, Debug)]
pub struct DecisionInput<'a> {
    pub classification: &'a Classification,
    pub location: &'a Location,
    pub dmarc_passed: bool,
    /// Locked by the user: never moved.
    pub locked: bool,
    /// The sender matches a trusted address or domain.
    pub trusted: bool,
    /// The user wrote to the sender, rescued or corrected one of its mails,
    /// or received enough legitimate authenticated mails from it.
    pub known_sender: bool,
}

/// Archives a new inbox mail of a thread the user muted, unless it is
/// locked.
///
/// It runs before any classification, so a mail Claude refused or never
/// saw is archived too. Muting is the user's own choice: the caller
/// applies this move even in dry-run mode.
pub fn muted_thread_move(location: &Location, locked: bool, thread_muted: bool) -> Option<Move> {
    (thread_muted && !locked && *location == Location::Inbox).then_some(Move::Archive)
}

/// Decides the move for a classified mail, or `None` to leave it where it
/// is. The rules apply in order:
///
/// 1. A locked mail is never moved.
/// 2. A mail in junk goes back to the inbox if the sender is trusted and
///    passed DMARC; otherwise it stays.
/// 3. A mail anywhere else but the inbox (archive, trash, sent, a user
///    folder) is not moved: it is where the user put it.
/// 4. A trusted sender's mail is not moved.
/// 5. Spam, or phishing, at or above the threshold goes to junk, unless
///    the policy protects known senders and this one is known and passed
///    DMARC.
/// 6. A category the policy archives is archived.
///
/// A mail the classifier refused has no classification: the caller does
/// not move it. Muted threads are handled before classification, by
/// [`muted_thread_move`].
pub fn decide(input: &DecisionInput<'_>, policy: &SpamPolicy) -> Option<Move> {
    let classification = input.classification;
    if input.locked {
        return None;
    }
    if *input.location == Location::Junk {
        return (input.trusted && input.dmarc_passed).then_some(Move::Inbox);
    }
    if *input.location != Location::Inbox || input.trusted {
        return None;
    }
    let suspicious = classification.category.is_tag(SPAM_KEY) || classification.is_phishing;
    if suspicious && classification.spam_confidence >= policy.threshold {
        let spared = policy.protect_known_senders && input.known_sender && input.dmarc_passed;
        return (!spared).then_some(Move::Junk);
    }
    let archived = match &classification.category {
        Category::Tag(key) => policy.archive.contains(key),
        Category::Unknown => false,
    };
    archived.then_some(Move::Archive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classification::{Category, Classification, Priority, SpamConfidence};
    use crate::mail::Location;
    use crate::tag::TagKey;

    /// An inbox mail in `work`, confidence 0, every flag off, under a
    /// policy with threshold 90, nothing archived, protection on.
    struct Fixture {
        classification: Classification,
        location: Location,
        dmarc_passed: bool,
        locked: bool,
        trusted: bool,
        known_sender: bool,
        policy: SpamPolicy,
    }

    fn key(k: &str) -> TagKey {
        TagKey::from_stored(k).unwrap()
    }

    fn fixture() -> Fixture {
        Fixture {
            classification: Classification {
                category: Category::Tag(key("work")),
                priority: Priority::Normal,
                needs_reply: false,
                is_phishing: false,
                spam_confidence: SpamConfidence::clamped(0),
                summary: String::new(),
                reason: String::new(),
            },
            location: Location::Inbox,
            dmarc_passed: false,
            locked: false,
            trusted: false,
            known_sender: false,
            policy: SpamPolicy {
                threshold: SpamConfidence::clamped(90),
                archive: Default::default(),
                protect_known_senders: true,
            },
        }
    }

    impl Fixture {
        fn spam(mut self, confidence: i64) -> Self {
            self.classification.category = Category::Tag(key("spam"));
            self.classification.spam_confidence = SpamConfidence::clamped(confidence);
            self
        }

        fn decide(&self) -> Option<Move> {
            decide(
                &DecisionInput {
                    classification: &self.classification,
                    location: &self.location,
                    dmarc_passed: self.dmarc_passed,
                    locked: self.locked,
                    trusted: self.trusted,
                    known_sender: self.known_sender,
                },
                &self.policy,
            )
        }
    }

    #[test]
    fn locked_never_moves() {
        let mut f = fixture().spam(100);
        f.locked = true;
        assert_eq!(f.decide(), None);
    }

    #[test]
    fn locked_in_junk_is_not_rescued() {
        let mut f = fixture();
        (f.location, f.trusted, f.dmarc_passed, f.locked) = (Location::Junk, true, true, true);
        assert_eq!(f.decide(), None);
    }

    #[test]
    fn junk_rescued_when_trusted_and_authenticated() {
        let mut f = fixture().spam(100);
        (f.location, f.trusted, f.dmarc_passed) = (Location::Junk, true, true);
        assert_eq!(f.decide(), Some(Move::Inbox));
    }

    #[test]
    fn junk_stays_without_dmarc() {
        let mut f = fixture();
        (f.location, f.trusted) = (Location::Junk, true);
        assert_eq!(f.decide(), None);
    }

    #[test]
    fn junk_stays_when_not_trusted() {
        let mut f = fixture();
        (f.location, f.dmarc_passed) = (Location::Junk, true);
        assert_eq!(f.decide(), None);
    }

    #[test]
    fn muted_thread_in_inbox_is_archived() {
        assert_eq!(
            muted_thread_move(&Location::Inbox, false, true),
            Some(Move::Archive)
        );
    }

    #[test]
    fn muted_thread_outside_inbox_is_not_moved() {
        assert_eq!(muted_thread_move(&Location::Archive, false, true), None);
        assert_eq!(muted_thread_move(&Location::Junk, false, true), None);
    }

    #[test]
    fn muted_thread_locked_is_not_moved() {
        assert_eq!(muted_thread_move(&Location::Inbox, true, true), None);
    }

    #[test]
    fn thread_not_muted_is_not_moved() {
        assert_eq!(muted_thread_move(&Location::Inbox, false, false), None);
    }

    #[test]
    fn mails_outside_the_inbox_are_not_moved() {
        let folder = Location::Folder(crate::mail::FolderPath::parse("Clients/Acme").unwrap());
        for location in [Location::Archive, Location::Trash, Location::Sent, folder] {
            let mut spam = fixture().spam(100);
            spam.location = location.clone();
            assert_eq!(spam.decide(), None, "spam in {location:?}");

            let mut newsletter = fixture();
            newsletter.policy.archive.insert(key("newsletter"));
            newsletter.classification.category = Category::Tag(key("newsletter"));
            newsletter.location = location.clone();
            assert_eq!(
                newsletter.decide(),
                None,
                "archive category in {location:?}"
            );
        }
    }

    #[test]
    fn trusted_sender_is_not_moved() {
        let mut f = fixture().spam(100);
        f.trusted = true;
        assert_eq!(f.decide(), None);
    }

    #[test]
    fn spam_at_threshold_goes_to_junk() {
        assert_eq!(fixture().spam(90).decide(), Some(Move::Junk));
        assert_eq!(fixture().spam(89).decide(), None);
    }

    #[test]
    fn phishing_needs_the_threshold() {
        let mut f = fixture();
        f.classification.is_phishing = true;
        f.classification.spam_confidence = SpamConfidence::clamped(50);
        assert_eq!(f.decide(), None);
        f.classification.spam_confidence = SpamConfidence::clamped(95);
        assert_eq!(f.decide(), Some(Move::Junk));
    }

    #[test]
    fn known_authenticated_sender_is_spared() {
        let mut f = fixture().spam(100);
        (f.known_sender, f.dmarc_passed) = (true, true);
        assert_eq!(f.decide(), None);
    }

    #[test]
    fn known_sender_without_dmarc_is_not_spared() {
        let mut f = fixture().spam(100);
        f.known_sender = true;
        assert_eq!(f.decide(), Some(Move::Junk));
    }

    #[test]
    fn protection_off_lets_threshold_decide() {
        let mut f = fixture().spam(100);
        (f.known_sender, f.dmarc_passed) = (true, true);
        f.policy.protect_known_senders = false;
        assert_eq!(f.decide(), Some(Move::Junk));
    }

    #[test]
    fn archive_category_is_archived() {
        let mut f = fixture();
        f.policy.archive.insert(key("newsletter"));
        f.classification.category = Category::Tag(key("newsletter"));
        assert_eq!(f.decide(), Some(Move::Archive));
    }

    #[test]
    fn unknown_category_is_not_moved() {
        let mut f = fixture();
        f.classification.category = Category::Unknown;
        assert_eq!(f.decide(), None);
    }
}
