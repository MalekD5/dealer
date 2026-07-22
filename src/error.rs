//! Shared error and result types.

/// A boxed error produced by any dealer subsystem.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// The result type used across dealer's subsystems.
pub type Result<T> = std::result::Result<T, Error>;

/// Builds an [`Error`] from a message.
pub fn error(message: impl Into<String>) -> Error {
    Error::from(message.into())
}
