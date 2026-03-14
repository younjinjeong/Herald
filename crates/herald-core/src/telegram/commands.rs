use teloxide::utils::command::BotCommands;

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
