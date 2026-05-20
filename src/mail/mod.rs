mod queue;
mod unclassified;

pub use queue::UnclassifiedMailQueue;
pub use unclassified::{MailMessageView, PersistMailRequest, list_unclassified, persist};
