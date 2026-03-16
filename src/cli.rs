use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use crate::ChronicleError;

#[derive(Clone, Debug, PartialEq, ValueEnum)]
pub enum Backend {
    Ollama,
    Claude,
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Backend::Ollama => write!(f, "ollama"),
            Backend::Claude => write!(f, "claude"),
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "git-chronicle",
    about = "Generate a human-readable narrative of a git repository's history using an LLM.",
    long_about = None,
)]
pub struct Cli {
    /// Path to the git repository to analyse [default: current directory]
    pub path: Option<PathBuf>,

    /// LLM backend to use
    #[arg(long, default_value_t = Backend::Ollama, value_enum)]
    pub backend: Backend,

    /// Model name to pass to the backend
    #[arg(long)]
    pub model: String,

    /// Number of commits or summaries per batch (minimum 2)
    #[arg(long, default_value_t = 5)]
    pub group_size: usize,

    /// Omit diffs from prompts to reduce token usage
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub no_diffs: bool,

    /// Directory containing custom batch.tera / reduce.tera templates
    #[arg(long)]
    pub template: Option<PathBuf>,

    /// Path to write the JSON audit trail (omit to skip)
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Filter commits by author (repeatable)
    #[arg(long, action = clap::ArgAction::Append)]
    pub author: Vec<String>,

    /// Filter commits after this date (YYYY-MM-DD)
    #[arg(long)]
    pub since: Option<String>,

    /// Branch to read history from [default: current branch]
    #[arg(long)]
    pub branch: Option<String>,

    /// Start commit SHA, 40-char hex (inclusive) [default: repo start]
    #[arg(long)]
    pub from: Option<String>,

    /// End commit SHA, 40-char hex (inclusive) [default: HEAD]
    #[arg(long)]
    pub to: Option<String>,
}

impl Cli {
    pub fn validate(&self) -> Result<(), ChronicleError> {
        if self.group_size < 2 {
            return Err(ChronicleError::InvalidConfig(
                "--group-size must be at least 2".to_string(),
            ));
        }

        if self.backend == Backend::Claude && std::env::var("ANTHROPIC_API_KEY").is_err() {
            return Err(ChronicleError::InvalidConfig(
                "--backend claude requires the ANTHROPIC_API_KEY environment variable to be set"
                    .to_string(),
            ));
        }

        if let Some(sha) = &self.from {
            validate_sha(sha, "--from")?;
        }
        if let Some(sha) = &self.to {
            validate_sha(sha, "--to")?;
        }

        if let Some(tmpl) = &self.template {
            if !tmpl.exists() {
                return Err(ChronicleError::InvalidConfig(format!(
                    "--template path '{}' does not exist",
                    tmpl.display()
                )));
            }
            if !tmpl.is_dir() {
                return Err(ChronicleError::InvalidConfig(format!(
                    "--template path '{}' is not a directory",
                    tmpl.display()
                )));
            }
        }

        let repo_path = match &self.path {
            Some(p) => p.clone(),
            None => std::env::current_dir().map_err(ChronicleError::IoError)?,
        };
        if !repo_path.join(".git").exists() {
            return Err(ChronicleError::InvalidConfig(format!(
                "'{}' does not appear to be a git repository (no .git directory found)",
                repo_path.display()
            )));
        }

        Ok(())
    }
}

fn validate_sha(sha: &str, flag: &str) -> Result<(), ChronicleError> {
    let is_valid = sha.len() == 40 && sha.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'));
    if !is_valid {
        return Err(ChronicleError::InvalidConfig(format!(
            "{flag} must be a 40-character lowercase hexadecimal SHA, got '{sha}'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn base_cli(path: PathBuf) -> Cli {
        Cli {
            path: Some(path),
            backend: Backend::Ollama,
            model: "test-model".to_string(),
            group_size: 10,
            no_diffs: false,
            template: None,
            output: None,
            author: vec![],
            since: None,
            branch: None,
            from: None,
            to: None,
        }
    }

    fn make_git_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir(dir.path().join(".git")).expect("create .git");
        dir
    }

    #[test]
    fn group_size_too_small() {
        let dir = make_git_dir();
        let cli = Cli {
            group_size: 1,
            ..base_cli(dir.path().to_path_buf())
        };
        let err = cli.validate().unwrap_err();
        assert!(
            err.to_string().contains("--group-size must be at least 2"),
            "got: {err}"
        );
    }

    #[test]
    fn group_size_minimum_accepted() {
        let dir = make_git_dir();
        let cli = Cli {
            group_size: 2,
            ..base_cli(dir.path().to_path_buf())
        };
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn claude_without_api_key() {
        let dir = make_git_dir();
        // Only run this test when the key is genuinely absent.
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            return;
        }
        let cli = Cli {
            backend: Backend::Claude,
            ..base_cli(dir.path().to_path_buf())
        };
        let err = cli.validate().unwrap_err();
        assert!(err.to_string().contains("ANTHROPIC_API_KEY"), "got: {err}");
    }

    #[test]
    fn from_non_hex_rejected() {
        let dir = make_git_dir();
        let cli = Cli {
            from: Some("not-a-sha".to_string()),
            ..base_cli(dir.path().to_path_buf())
        };
        let err = cli.validate().unwrap_err();
        assert!(err.to_string().contains("--from"), "got: {err}");
    }

    #[test]
    fn from_uppercase_hex_rejected() {
        let dir = make_git_dir();
        let cli = Cli {
            from: Some("A".repeat(40)),
            ..base_cli(dir.path().to_path_buf())
        };
        let err = cli.validate().unwrap_err();
        assert!(err.to_string().contains("--from"), "got: {err}");
    }

    #[test]
    fn from_valid_sha_accepted() {
        let dir = make_git_dir();
        let cli = Cli {
            from: Some("a".repeat(40)),
            ..base_cli(dir.path().to_path_buf())
        };
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn to_non_hex_rejected() {
        let dir = make_git_dir();
        let cli = Cli {
            to: Some("z".repeat(40)),
            ..base_cli(dir.path().to_path_buf())
        };
        let err = cli.validate().unwrap_err();
        assert!(err.to_string().contains("--to"), "got: {err}");
    }

    #[test]
    fn template_nonexistent_path_rejected() {
        let dir = make_git_dir();
        let cli = Cli {
            template: Some(PathBuf::from("/tmp/this_path_does_not_exist_chronicle")),
            ..base_cli(dir.path().to_path_buf())
        };
        let err = cli.validate().unwrap_err();
        assert!(err.to_string().contains("--template"), "got: {err}");
    }

    #[test]
    fn template_file_not_dir_rejected() {
        let dir = make_git_dir();
        let file = dir.path().join("not_a_dir.txt");
        fs::write(&file, b"").expect("write");
        let cli = Cli {
            template: Some(file),
            ..base_cli(dir.path().to_path_buf())
        };
        let err = cli.validate().unwrap_err();
        assert!(err.to_string().contains("--template"), "got: {err}");
    }

    #[test]
    fn template_valid_dir_accepted() {
        let dir = make_git_dir();
        let tmpl_dir = dir.path().join("templates");
        fs::create_dir(&tmpl_dir).expect("create templates dir");
        let cli = Cli {
            template: Some(tmpl_dir),
            ..base_cli(dir.path().to_path_buf())
        };
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn path_without_git_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cli = base_cli(dir.path().to_path_buf());
        let err = cli.validate().unwrap_err();
        assert!(
            err.to_string()
                .contains("does not appear to be a git repository"),
            "got: {err}"
        );
    }

    #[test]
    fn valid_git_repo_path_accepted() {
        let dir = make_git_dir();
        let cli = base_cli(dir.path().to_path_buf());
        assert!(cli.validate().is_ok());
    }
}
