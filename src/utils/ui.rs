use console::{style, Emoji};

// Emojis for consistent output
pub static SUCCESS: Emoji<'_, '_> = Emoji("✅ ", "");
pub static ERROR: Emoji<'_, '_> = Emoji("❌ ", "");
pub static NEW: Emoji<'_, '_> = Emoji("🆕 ", "");
pub static INFO: Emoji<'_, '_> = Emoji("ℹ️ ", "");

/// Print a success message
pub fn success(message: &str) {
    println!("{}{}", SUCCESS, style(message).green());
}

/// Print an error message
pub fn error(message: &str) {
    eprintln!("{}{}", ERROR, style(message).red());
}

/// Print an info message
pub fn info(message: &str) {
    println!("{}{}", INFO, style(message).blue());
}

/// Print a "new" message
pub fn new_item(message: &str) {
    println!("{}{}", NEW, style(message).yellow());
}

/// Print a dry-run message
pub fn dry_run(message: &str) {
    println!("{}{}", INFO, style(format!("[DRY RUN] {}", message)).cyan());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_functions() {
        // These functions just print, so we test they don't panic
        success("test success");
        error("test error");
        info("test info");
        new_item("test new");
        dry_run("test dry run");
    }
}