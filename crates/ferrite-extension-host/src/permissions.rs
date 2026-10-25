//! Permissions and the user's decisions about them.
//!
//! A manifest only *requests* permissions. An extension gets a permission
//! when the user grants it; the decisions are stored in a JSON file and
//! enforced on every host call. Until the user has decided about every
//! permission an extension requests, it is not started.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
  DocumentsRead,
  DocumentsWrite,
}

impl Permission {
  pub fn description(self) -> &'static str {
    match self {
      Self::DocumentsRead => "Read open documents and their changes, and show problems in them",
      Self::DocumentsWrite => "Edit open documents",
    }
  }
}

/// The user's decisions for one extension.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Decision {
  pub granted: BTreeSet<Permission>,
  pub denied: BTreeSet<Permission>,
  /// Turned off by the user.
  pub disabled: bool,
}

impl Decision {
  /// The requested permissions the user has granted.
  pub fn effective(&self, requested: &[Permission]) -> BTreeSet<Permission> {
    requested
      .iter()
      .copied()
      .filter(|permission| self.granted.contains(permission))
      .collect()
  }

  /// Requested permissions the user hasn't decided about yet.
  pub fn pending(&self, requested: &[Permission]) -> Vec<Permission> {
    requested
      .iter()
      .copied()
      .filter(|permission| !self.granted.contains(permission) && !self.denied.contains(permission))
      .collect()
  }
}

/// All decisions, by extension id, persisted to `path`.
#[derive(Debug, Default)]
pub struct Decisions {
  path: Option<PathBuf>,
  entries: BTreeMap<String, Decision>,
}

impl Decisions {
  /// Loads decisions from `path`. A missing or unreadable file means no
  /// decisions (unreadable ones are logged), so nothing is granted.
  pub fn load(path: Option<PathBuf>) -> Self {
    let entries = path
      .as_deref()
      .and_then(|path| match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
          .map_err(|error| tracing::warn!(%error, path = %path.display(), "ignoring invalid extension permissions file"))
          .ok(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
          tracing::warn!(%error, path = %path.display(), "could not read extension permissions");
          None
        }
      })
      .unwrap_or_default();
    Self { path, entries }
  }

  pub fn get(&self, id: &str) -> Decision {
    self.entries.get(id).cloned().unwrap_or_default()
  }

  /// Records a decision and saves the file.
  pub fn set(&mut self, id: &str, decision: Decision) {
    if decision == Decision::default() {
      self.entries.remove(id);
    } else {
      self.entries.insert(id.to_owned(), decision);
    }
    self.save();
  }

  fn save(&self) {
    let Some(path) = &self.path else {
      return;
    };
    let result = serde_json::to_string_pretty(&self.entries)
      .map_err(std::io::Error::other)
      .and_then(|text| write_atomically(path, text.as_bytes()));
    if let Err(error) = result {
      tracing::error!(%error, path = %path.display(), "could not save extension permissions");
    }
  }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let temporary = path.with_extension("json.tmp");
  std::fs::write(&temporary, bytes)?;
  std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_granted_requested_permissions_are_effective() {
    let decision = Decision {
      granted: BTreeSet::from([Permission::DocumentsRead]),
      denied: BTreeSet::from([Permission::DocumentsWrite]),
      disabled: false,
    };
    let requested = [Permission::DocumentsRead, Permission::DocumentsWrite];
    assert_eq!(
      decision.effective(&requested),
      BTreeSet::from([Permission::DocumentsRead])
    );
    assert!(decision.pending(&requested).is_empty());
    assert!(decision.effective(&[Permission::DocumentsWrite]).is_empty());
    assert_eq!(Decision::default().pending(&requested), requested.to_vec());
  }

  #[test]
  fn decisions_survive_a_reload_and_bad_files_grant_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("permissions.json");
    let mut decisions = Decisions::load(Some(path.clone()));
    let decision = Decision {
      granted: BTreeSet::from([Permission::DocumentsRead]),
      ..Decision::default()
    };
    decisions.set("example.tidy", decision.clone());
    assert_eq!(
      Decisions::load(Some(path.clone())).get("example.tidy"),
      decision
    );

    std::fs::write(&path, "{ not json").expect("write");
    assert_eq!(
      Decisions::load(Some(path)).get("example.tidy"),
      Decision::default()
    );
  }
}
