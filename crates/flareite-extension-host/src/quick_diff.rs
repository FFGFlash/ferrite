//! The host functions of the `quick-diff` interface (API 1.9): extensions
//! name the originals of files ([`crate::ExtensionHost::original_of`]) and
//! take changes applied to them ([`crate::ExtensionHost::apply_to_original`]).
//! The editor compares, marks, and reverts on its own.

use std::path::Path;

use crate::Notification;
use crate::bindings::flareite::extension::quick_diff::{Applied, Host};
use crate::bindings::flareite::extension::types::ApiError;
use crate::documents::MAX_PROVIDED_BYTES;
use crate::runtime::StoreState;
use crate::worker::Payload;

/// A request for a file's original, answered with `set-original`.
pub struct OriginalRequest {
  pub id: u64,
  pub path: String,
}

fn invalid(message: impl Into<String>) -> ApiError {
  ApiError::InvalidArgument(message.into())
}

impl Host for StoreState {
  fn requested_file(&mut self) -> Option<String> {
    match &self.api.payload {
      Payload::Original(request) => Some(request.path.clone()),
      _ => None,
    }
  }

  fn set_original(&mut self, path: String, uri: Option<String>) -> Result<(), ApiError> {
    let Payload::Original(request) = &self.api.payload else {
      return Err(invalid(
        "set-original answers a request: call it from the quick_diff original command, while requested-file() returns the file",
      ));
    };
    if request.path != path {
      return Err(invalid(format!(
        "the request is for {:?}, not {path:?}",
        request.path
      )));
    }
    if let Some(uri) = &uri {
      self.own_uri(uri)?;
    }
    let id = request.id;
    self.api.shared()?.provided(id, uri);
    Ok(())
  }

  fn to_apply(&mut self) -> Option<Applied> {
    match &self.api.payload {
      Payload::Apply(applied) => Some(applied.clone()),
      _ => None,
    }
  }

  fn originals_changed(&mut self, paths: Vec<String>) -> Result<(), ApiError> {
    if paths.len() > 1_000 {
      return Err(ApiError::TooLarge("at most 1,000 files at once".into()));
    }
    if let Some(path) = paths.iter().find(|path| !Path::new(path).is_absolute()) {
      return Err(invalid(format!("{path:?} isn't an absolute path")));
    }
    if self.api.manifest().contributions.quick_diff.is_none() {
      return Err(invalid("the manifest declares no [quick_diff]"));
    }
    self
      .api
      .shared()?
      .embedder
      .notify(Notification::OriginalsChanged(paths));
    Ok(())
  }

  fn apply_selected_ranges(&mut self) -> Result<(), ApiError> {
    if self
      .api
      .manifest()
      .contributions
      .quick_diff
      .as_ref()
      .is_none_or(|quick_diff| quick_diff.apply.is_none())
    {
      return Err(invalid("the manifest's [quick_diff] has no apply command"));
    }
    self
      .api
      .shared()?
      .embedder
      .notify(Notification::ApplySelectedRanges);
    Ok(())
  }
}

/// Checks a text to apply to an original.
pub fn check_applied(text: &str) -> Result<(), String> {
  if text.len() > MAX_PROVIDED_BYTES {
    Err(format!(
      "An original can be at most {} MiB.",
      MAX_PROVIDED_BYTES / 1024 / 1024
    ))
  } else {
    Ok(())
  }
}
