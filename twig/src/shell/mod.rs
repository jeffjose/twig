mod bash;
mod raw;
mod tcsh;
mod zsh;

pub use bash::BashFormatter;
pub use raw::RawFormatter;
pub use tcsh::TcshFormatter;
pub use zsh::ZshFormatter;

/// Trait for shell-specific ANSI escape code formatting
pub trait ShellFormatter {
    /// Format ANSI escape codes with shell-specific wrapping
    ///
    /// # Arguments
    /// * `ansi_code` - The ANSI escape sequence (e.g., "\x1b[36m")
    /// * `text` - The visible text to display
    /// * `reset_code` - The ANSI reset sequence (e.g., "\x1b[0m")
    ///
    /// # Returns
    /// Formatted string with shell-specific wrapping around ANSI codes
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String;

    /// Post-process the final output string for shell-specific requirements
    ///
    /// For example, TCSH and Zsh need literal `\n` instead of actual newlines.
    ///
    /// # Arguments
    /// * `output` - The complete formatted prompt string
    ///
    /// # Returns
    /// Post-processed string ready for the shell
    fn finalize(&self, output: &str) -> String {
        // Default implementation: no post-processing
        output.to_string()
    }
}

/// Shell output modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellMode {
    /// Raw ANSI codes with no wrapping (for --prompt flag)
    Raw,
    /// Bash format with \[...\] wrapping
    Bash,
    /// Zsh format with %{...%} wrapping
    Zsh,
    /// TCSH format with %{...%} wrapping
    Tcsh,
}

/// Factory function to create shell formatter based on mode
pub fn get_formatter(mode: ShellMode) -> Box<dyn ShellFormatter> {
    match mode {
        ShellMode::Raw => Box::new(RawFormatter),
        ShellMode::Bash => Box::new(BashFormatter),
        ShellMode::Zsh => Box::new(ZshFormatter),
        ShellMode::Tcsh => Box::new(TcshFormatter),
    }
}
