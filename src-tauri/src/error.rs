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
        Self::Win32(e.message())
    }
}

pub type AppResult<T> = Result<T, AppError>;
