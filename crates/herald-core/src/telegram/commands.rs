use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::command::BotCommands;

use crate::auth::chat_id::is_authorized;
use crate::telegram::bot::BotState;
use crate::telegram::callbacks::build_session_keyboard;
use crate::telegram::formatting::{format_session_list, format_status};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Herald commands:")]
pub enum HeraldCommand {
    #[command(description = "Start Herald and authenticate")]
    Start,
    #[command(description = "List active Claude Code sessions")]
    Sessions,
    #[command(description = "Show Herald daemon status")]
    Status,
    #[command(description = "Show help")]
    Help,
}

pub async fn command_handler(
    bot: Bot,
    msg: Message,
    cmd: HeraldCommand,
    state: BotState,
) -> ResponseResult<()> {
    match cmd {
        HeraldCommand::Start => handle_start(bot, msg, state).await,
        HeraldCommand::Sessions => handle_sessions(bot, msg, state).await,
        HeraldCommand::Status => handle_status(bot, msg, state).await,
        HeraldCommand::Help => handle_help(bot, msg, state).await,
    }
}

async fn handle_start(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let chat_id = msg.chat.id.0;
    let config = state.config.read().await;

    if is_authorized(&config, chat_id) {
        bot.send_message(
            msg.chat.id,
            "Welcome back to Herald! You are authenticated.\n\n\
             Use /sessions to see active Claude Code sessions.\n\
             Use /status to check daemon status.\n\
             Use /help for all commands.",
        )
        .await?;
    } else {
        bot.send_message(
            msg.chat.id,
            "Welcome to Herald - Claude Code Telegram Remote Control.\n\n\
             To authenticate, run `herald setup` in your terminal \
             and send the verification code here.",
        )
        .await?;
    }

    Ok(())
}

async fn handle_sessions(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let config = state.config.read().await;
    if !is_authorized(&config, msg.chat.id.0) {
        bot.send_message(msg.chat.id, "Unauthorized. Run `herald setup` first.")
            .await?;
        return Ok(());
    }
    drop(config);

    let sessions = state.registry.list().await;

    if sessions.is_empty() {
        bot.send_message(msg.chat.id, "No active Claude Code sessions.")
            .await?;
    } else {
        let keyboard = build_session_keyboard(&sessions);
        let text = format_session_list(&sessions);
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(keyboard)
            .await?;
    }

    Ok(())
}

async fn handle_status(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let config = state.config.read().await;
    if !is_authorized(&config, msg.chat.id.0) {
        bot.send_message(msg.chat.id, "Unauthorized. Run `herald setup` first.")
            .await?;
        return Ok(());
    }
    drop(config);

    let uptime = (chrono::Utc::now() - state.start_time).num_seconds() as u64;
    let session_count = state.registry.count().await;
    let connected = state
        .telegram_connected
        .load(std::sync::atomic::Ordering::Relaxed);

    let text = format_status(uptime, session_count, connected);
    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::MarkdownV2)
        .await?;

    Ok(())
}

async fn handle_help(bot: Bot, msg: Message, _state: BotState) -> ResponseResult<()> {
    let help_text = "Herald Commands:\n\n\
        /start - Connect to Herald\n\
        /sessions - List active Claude Code sessions\n\
        /status - Show daemon status\n\
        /help - Show this help\n\n\
        Send any text message to forward it as a prompt \
        to the selected Claude Code session.";

    bot.send_message(msg.chat.id, help_text).await?;
    Ok(())
}
