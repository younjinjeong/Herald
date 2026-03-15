// Message queue functionality has been moved to herald_core::telegram::bot
// This module is kept for any daemon-specific queue extensions.

pub use herald_core::telegram::bot::{drain_queue, enqueue_message, OutboundMessage};
