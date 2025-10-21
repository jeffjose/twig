fn main() {
    // Hardcoded format string (no config yet)
    let format = "{dir} $ ";

    // Get current working directory
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Simple string substitution (no colors yet)
    let output = format.replace("{dir}", &cwd);

    // Print it (no newline - this goes in a shell prompt)
    print!("{}", output);
}
