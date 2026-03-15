use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::command::BotCommands;

use crate::auth::chat_id::is_authorized;
use crate::telegram::bot::BotState;
use crate::telegram::callbacks::build_session_keyboard;
use crate::telegram::formatting::format_status;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Herald commands:")]
pub enum HeraldCommand {
    #[command(description = "Start Herald and authenticate")]
    Start,
    #[command(description = "List active Claude Code sessions")]
    Sessions,
    #[command(description = "Show Herald daemon status")]
    Status,
    #[command(description = "Show token usage across sessions")]
    Tokens,
    #[command(description = "Show recent conversation log")]
    Log,
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
        HeraldCommand::Tokens => handle_tokens(bot, msg, state).await,
        HeraldCommand::Log => handle_log(bot, msg, state).await,
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
        let active = state.active_session.lock().await;
        let active_id = active.clone();
        drop(active);

        let keyboard = build_session_keyboard(&sessions);
        let mut text = "Active Sessions:\n\n".to_string();
        for s in &sessions {
            let indicator = if active_id.as_deref() == Some(&s.id) {
                " \u{25c0} active"
            } else {
                ""
            };
            let mut mode_flags = Vec::new();
            if s.plan_mode {
                mode_flags.push("P");
            }
            if s.bypass_permissions {
                mode_flags.push("B");
            }
            let mode_str = if mode_flags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", mode_flags.join(","))
            };
            text.push_str(&format!(
                "{} ({}){}{}\n",
                s.tag(),
                s.cwd.split('/').last().unwrap_or(&s.cwd),
                mode_str,
                indicator
            ));
        }
        text.push_str("\nTap a session to select it.\nTip: @name or reply to target a session.");
        bot.send_message(msg.chat.id, text)
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

async fn handle_tokens(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let config = state.config.read().await;
    if !is_authorized(&config, msg.chat.id.0) {
        bot.send_message(msg.chat.id, "Unauthorized.").await?;
        return Ok(());
    }
    drop(config);

    let total = state.registry.total_token_usage().await;
    let sessions = state.registry.list().await;

    let mut text = format!(
        "Token Usage Summary\n\
         ━━━━━━━━━━━━━━━━━━━━\n\
         Total: {} in / {} out\n\
         Cache: {} read / {} created\n\
         Cost:  ${:.4}\n",
        format_number(total.input_tokens),
        format_number(total.output_tokens),
        format_number(total.cache_read_tokens),
        format_number(total.cache_creation_tokens),
        total.total_cost_usd,
    );

    if !sessions.is_empty() {
        text.push_str("\nPer session:\n");
        for s in &sessions {
            text.push_str(&format!(
                "  {} — {} in / {} out (${:.4})\n",
                s.id,
                format_number(s.token_usage.input_tokens),
                format_number(s.token_usage.output_tokens),
                s.token_usage.total_cost_usd,
            ));
        }
    }

    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

async fn handle_log(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let config = state.config.read().await;
    if !is_authorized(&config, msg.chat.id.0) {
        bot.send_message(msg.chat.id, "Unauthorized.").await?;
        return Ok(());
    }
    drop(config);

    let active = state.active_session.lock().await;
    let session_id = match active.as_ref() {
        Some(id) => id.clone(),
        None => {
            bot.send_message(
                msg.chat.id,
                "No session selected. Use /sessions first.",
            )
            .await?;
            return Ok(());
        }
    };
    drop(active);

    let log = state.registry.get_conversation_log(&session_id).await;
    if log.is_empty() {
        bot.send_message(msg.chat.id, "No conversation log yet.")
            .await?;
        return Ok(());
    }

    // Show last 10 entries
    let recent: Vec<_> = log.iter().rev().take(10).collect();
    let mut text = format!("Conversation Log (session: {})\n━━━━━━━━━━━━━━━━━━━━\n\n", session_id);
    for entry in recent.iter().rev() {
        let icon = match entry.entry_type.as_str() {
            "user_prompt" => "\u{1f464}",
            "assistant_response" => "\u{1f916}",
            "tool_summary" => "\u{1f527}",
            _ => "\u{2022}",
        };
        let time = entry.timestamp.format("%H:%M:%S");
        let content = if entry.content.len() > 200 {
            format!("{}...", &entry.content[..200])
        } else {
            entry.content.clone()
        };
        text.push_str(&format!("{} [{}] {}\n\n", icon, time, content));
    }

    // Truncate if too long for Telegram
    if text.len() > 4000 {
        text.truncate(4000);
        text.push_str("\n... (truncated)");
    }

    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

async fn handle_help(bot: Bot, msg: Message, _state: BotState) -> ResponseResult<()> {
    let help_text = "Herald Commands:\n\n\
        /start - Connect to Herald\n\
        /sessions - List active sessions (tap to select)\n\
        /status - Show daemon status\n\
        /tokens - Show token usage across sessions\n\
        /log - Show recent conversation log\n\
        /help - Show this help\n\n\
        Sending prompts:\n\
        \u{2022} Type text \u{2192} sends to active session\n\
        \u{2022} @session_name text \u{2192} sends to named session\n\
        \u{2022} Reply to a session message \u{2192} sends to that session";

    bot.send_message(msg.chat.id, help_text).await?;
    Ok(())
}
