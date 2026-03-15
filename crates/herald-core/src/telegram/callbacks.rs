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

pub fn build_permission_keyboard(request_id: &str) -> InlineKeyboardMarkup {
    let buttons = vec![vec![
        InlineKeyboardButton::callback(
            "\u{2705} Approve".to_string(),
            format!("approve:{}", request_id),
        ),
        InlineKeyboardButton::callback(
            "\u{274c} Deny".to_string(),
            format!("deny:{}", request_id),
        ),
    ]];
    InlineKeyboardMarkup::new(buttons)
}

pub fn parse_callback_data(data: &str) -> Option<(&str, &str)> {
    data.split_once(':')
}
