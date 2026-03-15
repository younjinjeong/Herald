use anyhow::Result;
use std::time::{Duration, Instant};
use teloxide::prelude::*;
use teloxide::types::UpdateKind;

use herald_core::auth::otp::{generate_otp, verify_otp};
use herald_core::config::HeraldConfig;

use crate::wizard::{print_step, prompt_secret};

pub async fn run() -> Result<()> {
    println!();
    println!("  Herald - Claude Code Telegram Relay");
    println!("  ====================================");
    println!();

    // Step 1: Bot token
    print_step(1, 4, "Bot Token");
    println!("  Enter your Telegram Bot Token (from @BotFather):");
    let token = prompt_secret("  Bot token");

    if token.is_empty() {
        return Err(anyhow::anyhow!("Bot token cannot be empty"));
    }

    // Step 2: Validate token
    print_step(2, 4, "Validating bot token...");
    let bot = Bot::new(&token);
    let me = bot
        .get_me()
        .await
        .map_err(|e| anyhow::anyhow!("Invalid bot token: {}", e))?;
    let bot_username = me.username();
    println!("  Bot: @{}", bot_username);

    // Step 3: Store token
    print_step(3, 4, "Storing bot token securely");
    HeraldConfig::set_bot_token(&token)?;
    println!("  Token stored");

    // Step 4: OTP verification
    print_step(4, 4, "Authentication");
    let (otp_code, mut otp_record) = generate_otp(6, 300, 3);
    println!();
    println!("  Send this code to @{} in Telegram:", bot_username);
    println!();
    println!("    >>> {} <<<", otp_code);
    println!();
    println!("  Waiting for verification (5 minute timeout)...");

    // Manual polling loop for OTP verification
    let mut offset = 0i64;
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut verified_chat_id: Option<i64> = None;

    loop {
        if Instant::now() > deadline {
            return Err(anyhow::anyhow!(
                "Verification timed out. Run `herald setup` again."
            ));
        }

        let updates = bot
            .get_updates()
            .offset(offset as i32)
            .timeout(10)
            .await;

        let updates = match updates {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!("Polling error: {}", e);
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        for update in updates {
            offset = update.id.as_offset() as i64;

            if let UpdateKind::Message(msg) = update.kind {
                if let Some(text) = msg.text() {
                    let text = text.trim();
                    match verify_otp(text, &mut otp_record) {
                        Ok(true) => {
                            let chat_id = msg.chat.id.0;
                            bot.send_message(
                                msg.chat.id,
                                "Verified! Herald is now connected.\n\n\
                                 The daemon will relay Claude Code session output here.\n\
                                 Use /help to see available commands.",
                            )
                            .await?;
                            verified_chat_id = Some(chat_id);
                        }
                        Ok(false) => {
                            bot.send_message(msg.chat.id, "Wrong code. Please try again.")
                                .await?;
                        }
                        Err(e) => {
                            bot.send_message(
                                msg.chat.id,
                                format!("Verification failed: {}. Run `herald setup` again.", e),
                            )
                            .await?;
                            return Err(anyhow::anyhow!("OTP verification failed: {}", e));
                        }
                    }
                }
            }
        }

        if let Some(chat_id) = verified_chat_id {
            // Save config
            let mut config = HeraldConfig::default();
            config.auth.allowed_chat_ids.push(chat_id);
            let config_path = HeraldConfig::default_path();
            config.save(&config_path)?;

            println!();
            println!("  Verified! (chat_id: {})", chat_id);
            println!("  Config written to {}", config_path.display());
            println!();
            println!("  ====================================");
            println!("  Setup complete!");
            println!();
            println!("  Next steps:");
            println!("    herald start   - Start the daemon");
            println!("    herald status  - Check connection");
            println!();
            return Ok(());
        }
    }
}
