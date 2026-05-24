use std::fmt::{Display, Formatter};
use std::path::Path;

/// Error type returned by the Mneme eval harness CLI.
#[derive(Debug)]
pub struct EvalError {
    message: String,
    exit_code: i32,
}

impl EvalError {
    pub(crate) fn invalid_cli(message: impl Into<String>) -> Self {
        Self {
            message: format_invalid_cli_message(message.into()),
            exit_code: 2,
        }
    }

    pub(crate) fn scenario(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
        }
    }

    pub(crate) fn io(action: &str, path: &Path, source: std::io::Error) -> Self {
        Self {
            message: format!("{action} {}: {source}", path.display()),
            exit_code: 1,
        }
    }

    pub(crate) fn parse(path: &Path, source: serde_yaml::Error) -> Self {
        Self {
            message: format!("parse scenario {}: {source}", path.display()),
            exit_code: 1,
        }
    }

    pub(crate) fn json(path: &Path, source: serde_json::Error) -> Self {
        Self {
            message: format!("serialize report {}: {source}", path.display()),
            exit_code: 1,
        }
    }

    pub(crate) fn parse_json(path: &Path, source: serde_json::Error) -> Self {
        Self {
            message: format!("parse JSON report {}: {source}", path.display()),
            exit_code: 1,
        }
    }

    #[must_use]
    /// Process exit code that matches the error category.
    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

fn format_invalid_cli_message(message: String) -> String {
    if message.contains("Run `mneme-eval help") {
        message
    } else {
        format!("{message}\nRun `mneme-eval help` or `mneme-eval help <command>` for usage.")
    }
}

impl Display for EvalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EvalError {}
