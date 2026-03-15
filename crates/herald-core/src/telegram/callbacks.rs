use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::types::SessionInfoDto;

pub fn build_session_keyboard(sessions: &[SessionInfoDto]) -> InlineKeyboardMarkup {
    let buttons: Vec<Vec<InlineKeyboardButton>> = sessions
        .iter()
        .map(|s| {
            let label = format!(
                "{} {}",
                s.tag(),
                s.cwd.split('/').last().unwrap_or(&s.cwd)
            );
            vec![InlineKeyboardButton::callback(
                label,
                format!("select_session:{}", s.id),
            )]
        })
        .collect();
    InlineKeyboardMarkup::new(buttons)
}

pub fn parse_callback_data(data: &str) -> Option<(&str, &str)> {
    data.split_once(':')
}
