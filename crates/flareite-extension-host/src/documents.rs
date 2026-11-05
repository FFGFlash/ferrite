//! The host functions of the `documents` interface (API 1.8): read-only
//! documents extensions provide for addresses with their schemes, and tabs
//! comparing two documents.
//!
//! When the editor needs the text at an address, the host runs the
//! provider's command ([`crate::ExtensionHost::provide_text`]); the command
//! answers with `provide`, which the host keeps until the request reads it.
//! An extension can answer, open, and announce changes only for its own
//! schemes; a diff may name any provider's address, or any file.

use std::path::Path;

use crate::bindings::flareite::extension::documents::{Host, MergeOptions, MergeSide};
use crate::bindings::flareite::extension::types::ApiError;
use crate::runtime::StoreState;
use crate::worker::Payload;
use crate::{MergeRequest, Notification};

/// Largest text an extension provides.
pub const MAX_PROVIDED_BYTES: usize = 32 * 1024 * 1024;
const MAX_URI_BYTES: usize = 4 * 1024;

/// An address's scheme: what's before its first `:`, if it's a valid
/// scheme.
pub fn scheme_of(uri: &str) -> Option<&str> {
  let (scheme, _) = uri.split_once(':')?;
  crate::contributions::valid_scheme(scheme).then_some(scheme)
}

fn invalid(message: String) -> ApiError {
  ApiError::InvalidArgument(message)
}

impl StoreState {
  /// Checks that `uri` is an address with one of the extension's schemes.
  pub(crate) fn own_uri(&self, uri: &str) -> Result<(), ApiError> {
    if uri.len() > MAX_URI_BYTES || uri.contains(char::is_control) {
      return Err(invalid(
        "addresses are at most 4 KiB, without control characters".into(),
      ));
    }
    let own = scheme_of(uri).is_some_and(|scheme| {
      self
        .api
        .manifest()
        .contributions
        .text_provider(scheme)
        .is_some()
    });
    if own {
      Ok(())
    } else {
      Err(invalid(format!(
        "{uri:?} isn't an address with one of this extension's schemes ([[text_providers]])"
      )))
    }
  }

  /// Checks one side of a diff: an absolute path, or an address some
  /// extension provides.
  fn side(&self, side: &str) -> Result<(), ApiError> {
    if Path::new(side).is_absolute() {
      return Ok(());
    }
    let provided = scheme_of(side).is_some_and(|scheme| {
      self
        .api
        .shared()
        .is_ok_and(|shared| shared.provides(scheme))
    });
    if provided && side.len() <= MAX_URI_BYTES {
      Ok(())
    } else {
      Err(invalid(format!(
        "{side:?} is neither an absolute path nor an address an extension provides"
      )))
    }
  }
}

impl Host for StoreState {
  fn requested(&mut self) -> Option<String> {
    match &self.api.payload {
      Payload::Provide(request) => Some(request.uri.clone()),
      _ => None,
    }
  }

  fn provide(&mut self, uri: String, text: Option<String>) -> Result<(), ApiError> {
    let Payload::Provide(request) = &self.api.payload else {
      return Err(invalid(
        "provide answers a request: call it from a text provider's command, while requested() returns the address".into(),
      ));
    };
    if request.uri != uri {
      return Err(invalid(format!(
        "the request is for {:?}, not {uri:?}",
        request.uri
      )));
    }
    if text
      .as_ref()
      .is_some_and(|text| text.len() > MAX_PROVIDED_BYTES)
    {
      return Err(ApiError::TooLarge(format!(
        "a provided text is at most {} MiB",
        MAX_PROVIDED_BYTES / 1024 / 1024
      )));
    }
    self.api.shared()?.provided(request.id, text);
    Ok(())
  }

  fn changed(&mut self, uris: Vec<String>) -> Result<(), ApiError> {
    if uris.len() > 1_000 {
      return Err(ApiError::TooLarge("at most 1,000 addresses at once".into()));
    }
    for uri in &uris {
      self.own_uri(uri.trim_end_matches('*'))?;
    }
    if !uris.is_empty() {
      self
        .api
        .shared()?
        .embedder
        .notify(Notification::ProvidedChanged(uris));
    }
    Ok(())
  }

  fn open(&mut self, uri: String) -> Result<(), ApiError> {
    self.own_uri(&uri)?;
    self
      .api
      .shared()?
      .embedder
      .notify(Notification::OpenProvided(uri));
    Ok(())
  }

  fn open_merge(&mut self, options: MergeOptions) -> Result<(), ApiError> {
    self.side(&options.base)?;
    self.side(&options.incoming.uri)?;
    self.side(&options.current.uri)?;
    if !Path::new(&options.result_file).is_absolute() {
      return Err(invalid("a merge's result is an absolute path".into()));
    }
    let title = options.title.trim();
    if title.is_empty() || title.chars().count() > 200 {
      return Err(invalid("a merge's title is 1 to 200 characters".into()));
    }
    self.api.own_command(options.on_complete.as_ref())?;
    let side = |side: MergeSide| -> Result<crate::MergeSide, ApiError> {
      let title = side.title.trim();
      if title.is_empty() || title.chars().count() > 100 {
        return Err(invalid(
          "a merge side's title is 1 to 100 characters".into(),
        ));
      }
      Ok(crate::MergeSide {
        uri: side.uri,
        title: title.to_owned(),
        description: side
          .description
          .map(|description| description.chars().take(200).collect()),
      })
    };
    let request = MergeRequest {
      base: options.base,
      incoming: side(options.incoming)?,
      current: side(options.current)?,
      result: options.result_file,
      title: title.to_owned(),
      reset_result: options.reset_result,
      on_complete: options.on_complete,
    };
    self
      .api
      .shared()?
      .embedder
      .notify(Notification::OpenMerge(request));
    Ok(())
  }

  fn open_diff(
    &mut self,
    original: String,
    modified: String,
    title: String,
  ) -> Result<(), ApiError> {
    self.side(&original)?;
    self.side(&modified)?;
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 200 {
      return Err(invalid("a diff's title is 1 to 200 characters".into()));
    }
    self.api.shared()?.embedder.notify(Notification::OpenDiff {
      original,
      modified,
      title: title.to_owned(),
    });
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn schemes_are_read_from_addresses() {
    assert_eq!(scheme_of("git:HEAD:src/main.rs"), Some("git"));
    assert_eq!(scheme_of("git::src/main.rs"), Some("git"));
    assert_eq!(scheme_of("C:/repo/main.rs"), None, "a drive isn't a scheme");
    assert_eq!(scheme_of("file:/a"), None, "reserved");
    assert_eq!(scheme_of("no scheme"), None);
  }
}
