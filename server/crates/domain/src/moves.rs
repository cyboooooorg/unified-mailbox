//! Where the code may move a mail.

use crate::mail::FolderPath;

/// A move the code may apply to a mail.
///
/// There is deliberately no `Delete`: nothing in Unified Mailbox deletes a
/// mail permanently. Spam goes to the provider's junk folder, confirmed
/// spam to its trash, where the provider purges it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Move {
    Inbox,
    Archive,
    Junk,
    Trash,
    Folder(FolderPath),
}
