//! The host functions of the `explorer` interface (API 1.6): marks on
//! files and folders in the explorer, such as a version control status.
//!
//! An extension replaces all its marks at once. Each is checked: badges are
//! one or two characters, colors are ones the manifest declares (sent on
//! as their theme tokens, which themes can change), and paths are made
//! absolute in the platform's form, so `C:/a/b` marks the explorer's
//! `C:\a\b`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use serde::{Serialize, Serializer};

use crate::bindings::flareite::extension::explorer::{FileMark as WitFileMark, Host};
use crate::bindings::flareite::extension::types::ApiError;
use crate::runtime::StoreState;

/// Most marks one extension shows.
pub const MAX_MARKS: usize = 100_000;
const MAX_TOOLTIP_CHARS: usize = 200;

/// A mark on a file or folder in the explorer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMark {
  pub path: String,
  /// One or two characters.
  pub badge: Option<String>,
  /// The color's theme token, `<extension id>.<name>`.
  pub color: Option<String>,
  pub tooltip: Option<String>,
  pub faded: bool,
  /// Also marks everything inside a folder.
  pub inherit: bool,
  /// Folders that contain it show its color and a dot.
  pub propagate: bool,
  pub priority: u8,
}

/// One extension's marks.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionMarks {
  /// The extension's id.
  pub extension: String,
  #[serde(serialize_with = "shared_marks")]
  pub marks: Arc<Vec<FileMark>>,
}

fn shared_marks<S: Serializer>(marks: &Arc<Vec<FileMark>>, s: S) -> Result<S::Ok, S::Error> {
  marks.serialize(s)
}

fn invalid(message: String) -> ApiError {
  ApiError::InvalidArgument(message)
}

impl Host for StoreState {
  fn set_file_marks(&mut self, marks: Vec<WitFileMark>) -> Result<(), ApiError> {
    if marks.len() > MAX_MARKS {
      return Err(ApiError::TooLarge(format!(
        "at most {MAX_MARKS} marks can be shown"
      )));
    }
    let api = &self.api;
    let colors = &api.manifest().contributions;
    let mut seen = HashSet::with_capacity(marks.len());
    let mut checked = Vec::with_capacity(marks.len());
    for mark in marks {
      let given = Path::new(&mark.path);
      if !given.is_absolute() {
        return Err(invalid(format!("{:?} is not an absolute path", mark.path)));
      }
      let path = std::path::absolute(given)
        .map_err(|error| invalid(format!("{:?}: {error}", mark.path)))?
        .display()
        .to_string();
      if !seen.insert(path.clone()) {
        return Err(invalid(format!("{path} is marked twice")));
      }
      if let Some(badge) = &mark.badge
        && (!(1..=2).contains(&badge.chars().count())
          || badge.chars().any(|c| c.is_control() || c.is_whitespace()))
      {
        return Err(invalid(format!(
          "badge {badge:?} must be one or two characters"
        )));
      }
      let color = match &mark.color {
        Some(name) => Some(
          colors
            .color(name)
            .ok_or_else(|| invalid(format!("no color {name:?} is declared")))?
            .id
            .clone(),
        ),
        None => None,
      };
      checked.push(FileMark {
        path,
        badge: mark.badge,
        color,
        tooltip: mark
          .tooltip
          .map(|tooltip| tooltip.chars().take(MAX_TOOLTIP_CHARS).collect()),
        faded: mark.faded,
        inherit: mark.inherit,
        propagate: mark.propagate,
        priority: mark.priority,
      });
    }
    api.shared()?.set_file_marks(api.extension_id(), checked);
    Ok(())
  }
}
