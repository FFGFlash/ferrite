use flareite_core::CoreError;
use flareite_workspace::FileError;
use serde::Serialize;

/// Error returned to the frontend: a stable machine-readable `code` and a
/// message suitable for showing to the user.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
  pub code: &'static str,
  pub message: String,
}

impl CommandError {
  pub fn new(code: &'static str, message: impl Into<String>) -> Self {
    Self {
      code,
      message: message.into(),
    }
  }

  pub fn internal(message: impl std::fmt::Display) -> Self {
    tracing::error!(%message, "internal command error");
    Self::new("internal", "Something went wrong. See the log for details.")
  }
}

impl From<CoreError> for CommandError {
  fn from(error: CoreError) -> Self {
    let code = match error {
      CoreError::UnknownDocument(_) => "unknownDocument",
      CoreError::Edit(_) => "invalidEdit",
      CoreError::Search(_) => "invalidSearch",
      CoreError::ReadOnly(_) => "readOnly",
    };
    tracing::warn!(%error, code, "editor command failed");
    Self::new(code, error.to_string())
  }
}

impl From<FileError> for CommandError {
  fn from(error: FileError) -> Self {
    let code = match error {
      FileError::NotFound { .. } => "notFound",
      FileError::PermissionDenied { .. } => "permissionDenied",
      FileError::IsDirectory { .. } => "isDirectory",
      FileError::AlreadyExists { .. } => "alreadyExists",
      FileError::Binary { .. } => "binary",
      FileError::InvalidText { .. } => "invalidText",
      FileError::LossyEncoding { .. } => "lossyEncoding",
      FileError::Unencodable { .. } => "unencodable",
      FileError::ModifiedOnDisk { .. } => "modifiedOnDisk",
      FileError::Io { .. } => "io",
    };
    tracing::warn!(%error, code, "file operation failed");
    Self::new(code, error.to_string())
  }
}
