use dialoguer::Password;

pub fn prompt_secret(label: &str) -> String {
    Password::new()
        .with_prompt(label)
        .interact()
        .unwrap_or_default()
}

pub fn print_step(step: u32, total: u32, message: &str) {
    println!("  Step {}/{}: {}", step, total, message);
}
