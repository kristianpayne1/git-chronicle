use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChronicleError {
    #[error("Could not access the git repository: {0}")]
    GitError(#[from] git2::Error),

    #[error("The language model failed to produce a response: {0}")]
    LlmFailure(String),

    #[error("Could not render the output template: {0}")]
    TemplateError(#[from] tera::Error),

    #[error("Could not read or write a file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Could not serialize or deserialize data: {0}")]
    SerializationError(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_failure_display() {
        let err = ChronicleError::LlmFailure("timed out after 3 retries".to_string());
        assert_eq!(
            err.to_string(),
            "The language model failed to produce a response: timed out after 3 retries"
        );
    }

    #[test]
    fn invalid_config_display() {
        let err = ChronicleError::InvalidConfig("--group-size must be at least 2".to_string());
        assert_eq!(
            err.to_string(),
            "Invalid configuration: --group-size must be at least 2"
        );
    }

    #[test]
    fn io_error_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let err = ChronicleError::from(io_err);
        assert!(
            err.to_string()
                .starts_with("Could not read or write a file:"),
            "got: {err}"
        );
    }

    #[test]
    fn git_error_display() {
        let git_err = git2::Error::from_str("not a git repository");
        let err = ChronicleError::from(git_err);
        assert!(
            err.to_string()
                .starts_with("Could not access the git repository:"),
            "got: {err}"
        );
    }

    #[test]
    fn template_error_display() {
        let tera = tera::Tera::default();
        let tera_err = tera
            .render("nonexistent_template", &tera::Context::new())
            .unwrap_err();
        let err = ChronicleError::from(tera_err);
        assert!(
            err.to_string()
                .starts_with("Could not render the output template:"),
            "got: {err}"
        );
    }

    #[test]
    fn serialization_error_display() {
        let json_err = serde_json::from_str::<serde_json::Value>("{ bad json }")
            .unwrap_err();
        let err = ChronicleError::from(json_err);
        assert!(
            err.to_string()
                .starts_with("Could not serialize or deserialize data:"),
            "got: {err}"
        );
    }

    #[test]
    fn usable_as_box_dyn_error() {
        let err: Box<dyn std::error::Error> =
            Box::new(ChronicleError::LlmFailure("test error".to_string()));
        assert!(!err.to_string().is_empty());
    }
}
