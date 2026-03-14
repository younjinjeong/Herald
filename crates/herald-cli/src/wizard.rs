use dialoguer::{Confirm, Input, Password};

pub fn prompt_input(label: &str) -> String {
    Input::new()
        .with_prompt(label)
        .interact_text()
        .unwrap_or_default()
}

pub fn prompt_secret(label: &str) -> String {
    Password::new()
        .with_prompt(label)
        .interact()
        .unwrap_or_default()
}

pub fn confirm(question: &str) -> bool {
    Confirm::new()
        .with_prompt(question)
        .default(true)
        .interact()
        .unwrap_or(false)
}

pub fn print_step(step: u32, total: u32, message: &str) {
    println!("  Step {}/{}: {}", step, total, message);
}
