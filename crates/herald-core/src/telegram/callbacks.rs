use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::types::SessionInfoDto;

pub fn build_session_keyboard(sessions: &[SessionInfoDto]) -> InlineKeyboardMarkup {
    let buttons: Vec<Vec<InlineKeyboardButton>> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            vec![InlineKeyboardButton::callback(
                format!("#{} {}", i + 1, s.cwd.split('/').last().unwrap_or(&s.cwd)),
                format!("select_session:{}", s.id),
            )]
        })
        .collect();
    InlineKeyboardMarkup::new(buttons)
}

pub fn parse_callback_data(data: &str) -> Option<(&str, &str)> {
    data.split_once(':')
}
