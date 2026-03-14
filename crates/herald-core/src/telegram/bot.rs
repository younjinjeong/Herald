use teloxide::prelude::*;

pub fn create_bot(token: &str) -> Bot {
    Bot::new(token)
}
