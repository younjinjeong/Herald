use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::types::{SessionInfoDto, SessionModes};

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

pub fn build_session_actions_keyboard(
    session_id: &str,
    modes: &SessionModes,
) -> InlineKeyboardMarkup {
    let plan_label = if modes.plan_mode {
        "\u{1f4cb} Plan: ON"
    } else {
        "\u{1f4cb} Plan: OFF"
    };
    let bypass_label = if modes.bypass_permissions {
        "\u{26a1} Bypass: ON"
    } else {
        "\u{26a1} Bypass: OFF"
    };
    let buttons = vec![vec![
        InlineKeyboardButton::callback(
            plan_label.to_string(),
            format!("toggle_plan:{}", session_id),
        ),
        InlineKeyboardButton::callback(
            bypass_label.to_string(),
            format!("toggle_bypass:{}", session_id),
        ),
    ]];
    InlineKeyboardMarkup::new(buttons)
}

pub fn parse_callback_data(data: &str) -> Option<(&str, &str)> {
    data.split_once(':')
}
