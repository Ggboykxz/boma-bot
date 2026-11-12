//! Errors raised by a use case.

use boma_domain::DomainError;

/// A use case failed.
///
/// The variant names the *layer* that failed, not the transport: the `api` crate maps these to a
/// stable error envelope, and the mapping is the only place that knows about HTTP status codes.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A pure business rule rejected the command.
    #[error(transparent)]
    Rule(#[from] DomainError),
    /// The request conflicts with what already exists: the address is taken, the name is used.
    #[error("conflict")]
    Conflict,
    /// The credentials do not match an account.
    #[error("invalid credentials")]
    InvalidCredentials,
    /// A required dependency did not answer.
    #[error("a dependency is unavailable")]
    Unavailable,
    /// The caller asked for something that does not exist, including another tenant's data.
    #[error("not found")]
    NotFound,
}

impl AppError {
    /// Whether the failure is caused by the caller rather than by the deployment.
    #[must_use]
    pub const fn is_client_fault(&self) -> bool {
        matches!(
            self,
            Self::Rule(_) | Self::Conflict | Self::InvalidCredentials | Self::NotFound
        )
    }
}

#[cfg(test)]
mod tests {
    use boma_domain::{DomainError, RunStatus};

    use super::AppError;

    #[test]
    fn req_012_a_rule_violation_survives_the_layer_boundary() {
        let rule = DomainError::invalid_transition(RunStatus::Completed, RunStatus::Running);
        let error = AppError::from(rule);
        assert!(error.is_client_fault());
        assert!(error.to_string().contains("completed"));
        assert!(!AppError::Unavailable.is_client_fault());
    }

    #[test]
    fn req_040_a_taken_address_is_a_conflict_and_not_a_server_error() {
        let error = AppError::Conflict;
        assert!(error.is_client_fault());
        assert!(error.is_client_fault());
        assert_eq!(error.to_string(), "conflict");
    }

    #[test]
    fn req_040_wrong_credentials_are_a_client_fault() {
        assert!(AppError::InvalidCredentials.is_client_fault());
        assert!(!AppError::Unavailable.is_client_fault());
    }
}
