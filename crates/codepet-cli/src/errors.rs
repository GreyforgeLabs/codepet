//! Maps storage and protocol failures onto protocol error responses.
use codepet_protocol::{ErrorCode, ProtocolError, Response};
use codepet_storage::StorageError;

/// A failed request: protocol code, bounded message and the `retryable` hint.
#[derive(Debug)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}
impl Failure {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: code.retryable(),
        }
    }
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }
    pub fn into_response(self, id: Option<String>) -> Response {
        let mut response = Response::err(id, self.code, self.message);
        if let Some(error) = response.error.as_mut() {
            error.retryable = self.retryable;
        }
        response
    }
}
impl From<ProtocolError> for Failure {
    fn from(e: ProtocolError) -> Self {
        Self::new(e.code, e.message)
    }
}
impl From<StorageError> for Failure {
    /// Every storage error has exactly one protocol code. Storage-unavailable failures are
    /// retryable only for SQLite lock contention, not for unsafe paths or corruption.
    fn from(e: StorageError) -> Self {
        let code = ErrorCode::parse(e.code()).unwrap_or(ErrorCode::Internal);
        let retryable = match code {
            ErrorCode::StorageUnavailable => e.retryable(),
            other => other.retryable(),
        };
        Self {
            code,
            message: e.to_string(),
            retryable,
        }
    }
}
impl From<serde_json::Error> for Failure {
    fn from(_: serde_json::Error) -> Self {
        Self::new(ErrorCode::Internal, "failed to encode result")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_reason_limit_is_shared() {
        assert_eq!(
            codepet_protocol::MAX_OVERRIDE_REASON_BYTES,
            codepet_core::MAX_OVERRIDE_REASON_BYTES
        );
        assert_eq!(
            codepet_storage::MAX_OVERRIDE_REASON_BYTES,
            codepet_core::MAX_OVERRIDE_REASON_BYTES
        );
    }

    #[test]
    fn lifecycle_vocabularies_are_shared() {
        assert_eq!(
            codepet_protocol::PET_IDS,
            codepet_core::species::SPECIES_IDS,
            "protocol pet ids equal core species ids, in order"
        );
        for mode in codepet_protocol::LIFE_MODES {
            let parsed = codepet_core::lifecycle::Mode::parse(mode).expect(mode);
            assert_eq!(parsed.as_str(), mode);
        }
        for pace in codepet_protocol::LIFE_PACES {
            let parsed = codepet_core::lifecycle::Pace::parse(pace).expect(pace);
            assert_eq!(parsed.as_str(), pace);
        }
        assert_eq!(
            codepet_protocol::MAX_MEMORIAL_LIMIT,
            codepet_storage::MAX_MEMORIAL_PAGE
        );
    }

    #[test]
    fn every_storage_error_maps_to_a_protocol_code() {
        let cases: Vec<(StorageError, ErrorCode, bool)> = vec![
            (StorageError::Busy, ErrorCode::Busy, true),
            (StorageError::NoActivePet, ErrorCode::NoActivePet, false),
            (StorageError::PassedAway, ErrorCode::NoActivePet, false),
            (
                StorageError::InvalidState("x"),
                ErrorCode::InvalidState,
                false,
            ),
            (StorageError::StaleRevision, ErrorCode::StaleRevision, false),
            (
                StorageError::UnsupportedProvider,
                ErrorCode::UnsupportedProvider,
                false,
            ),
            (StorageError::NotFound, ErrorCode::NotFound, false),
            (StorageError::TokenInvalid, ErrorCode::TokenInvalid, false),
            (StorageError::TokenExpired, ErrorCode::TokenExpired, false),
            (StorageError::ReviewChanged, ErrorCode::ReviewChanged, false),
            (StorageError::ReviewDamaged, ErrorCode::ReviewChanged, false),
            (
                StorageError::AcknowledgementRequired,
                ErrorCode::AcknowledgementRequired,
                false,
            ),
            (
                StorageError::OverrideRequired,
                ErrorCode::OverrideRequired,
                false,
            ),
            (
                StorageError::IdempotencyConflict,
                ErrorCode::IdempotencyConflict,
                false,
            ),
            (
                StorageError::InvalidParams("x"),
                ErrorCode::InvalidParams,
                false,
            ),
            (
                StorageError::InvalidPath("x"),
                ErrorCode::InvalidParams,
                false,
            ),
            (StorageError::Capacity, ErrorCode::Capacity, true),
            (
                StorageError::ResyncRequired,
                ErrorCode::ResyncRequired,
                false,
            ),
            (
                StorageError::SchemaUnsupported(9),
                ErrorCode::SchemaUnsupported,
                false,
            ),
            (StorageError::Corrupt, ErrorCode::StorageUnavailable, false),
            (
                StorageError::UnsafeStateDir("x".into()),
                ErrorCode::StorageUnavailable,
                false,
            ),
            (
                StorageError::Unavailable("x"),
                ErrorCode::StorageUnavailable,
                false,
            ),
            (
                StorageError::Io(std::io::Error::other("x")),
                ErrorCode::StorageUnavailable,
                false,
            ),
        ];
        for (error, code, retryable) in cases {
            let text = error.to_string();
            let f = Failure::from(error);
            assert_eq!((f.code, f.retryable), (code, retryable), "{text}");
            let r = f.into_response(Some("r1".into()));
            let body = r.error.unwrap();
            assert_eq!((body.code, body.retryable), (code, retryable));
            assert!(!r.ok);
        }
    }
}
