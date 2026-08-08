use std::fmt;

/// The single error type crossing the IPC boundary.
///
/// Commands return `Result<T, AppError>` and serialise to a plain string, so the
/// frontend can always render something useful without learning a shape. Nothing
/// here ever carries a raw registry path or a full filesystem path back to the
/// webview — messages are written for a person, not for a debugger.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),

    #[error("Windows rejected the request: {0}")]
    Win32(String),

    #[error("Could not read or write CursorForge's storage: {0}")]
    Storage(String),

    #[error("That file isn't something CursorForge can use: {0}")]
    Invalid(String),

    #[error("{0} is not a cursor role CursorForge knows about.")]
    UnknownRole(String),

    #[error("That path is outside CursorForge's storage and was refused.")]
    PathEscape,

    #[error("The image is too large to process safely ({0}).")]
    ImageTooLarge(String),

    #[error("No cursor pack with that id is installed.")]
    UnknownPack,

    #[error("No preset with that id exists.")]
    UnknownPreset,
}

impl AppError {
    pub fn msg(m: impl fmt::Display) -> Self {
        Self::Message(m.to_string())
    }

    pub fn invalid(m: impl fmt::Display) -> Self {
        Self::Invalid(m.to_string())
    }

    pub fn storage(m: impl fmt::Display) -> Self {
        Self::Storage(m.to_string())
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::Storage(format!("malformed data file: {e}"))
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for AppError {
    fn from(e: windows::core::Error) -> Self {
        Self::Win32(describe_win32(&e))
    }
}

/// Turns a Win32 error into something worth showing a person.
///
/// Some APIs report failure without setting an error code, so formatting the
/// code verbatim produces the nonsense "Windows rejected the request: The
/// operation completed successfully." Telling somebody an operation failed *and*
/// succeeded in the same sentence is worse than admitting we do not know why.
#[cfg(windows)]
pub fn describe_win32(e: &windows::core::Error) -> String {
    let message = e.message();
    let trimmed = message.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("The operation completed successfully.") {
        return "Windows refused it without saying why".to_owned();
    }
    trimmed.to_owned()
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_success_code_on_a_failure_path_is_not_reported_as_success() {
        // Some APIs report failure without setting an error code. Formatting it
        // verbatim tells the user the operation failed *and* completed
        // successfully, in one sentence.
        let bogus = windows::core::Error::from_hresult(windows::core::HRESULT(0));
        let described = describe_win32(&bogus);
        assert!(
            !described.to_lowercase().contains("completed successfully"),
            "got: {described}"
        );
        assert!(!described.is_empty());
    }

    #[test]
    fn a_real_error_keeps_its_own_message() {
        // ERROR_FILE_NOT_FOUND, as an HRESULT.
        let real = windows::core::Error::from_hresult(windows::core::HRESULT(-2147024894));
        let described = describe_win32(&real);
        assert!(!described.is_empty());
        assert!(!described.to_lowercase().contains("completed successfully"));
    }
}
