//! The client-completion oracle: a first-class "did every request eventually get
//! answered" assertion, populated by wrapping real client submissions (2f+1, F4).

pub mod client_pool;
pub mod completion_ledger;
pub mod reply_log;
pub mod vote_log;

pub use client_pool::ClientPool;
pub use completion_ledger::{CompletionLedger, RequestState};
pub use reply_log::ReplyLog;
pub use vote_log::VoteLog;
