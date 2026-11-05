//! Extension packages (`extension.zip`) and the extensions folder: unpacking
//! and checking a package, then moving it into place, and removing an
//! installed extension.
//!
//! A package is the extension's folder, zipped: `extension.toml` at its root
//! (or in its one top-level folder), with an entrypoint inside it. It's
//! unpacked into a staging folder beside the extensions folder, checked as an
//! installed extension's manifest is, and only then swapped in, so a failed
//! install leaves the extension that was there.

use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flareite_extension_host::MANIFEST_FILE;
use flareite_extension_host::manifest;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::InstallError;
use crate::github::Repo;

/// Where an extension was installed from, kept in its folder.
pub const SOURCE_FILE: &str = ".flareite-install.json";
/// Most files in a package.
pub const MAX_FILES: usize = 10_000;
/// Most bytes a package unpacks to.
pub const MAX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;

/// Where an installed extension came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
  pub repository: Repo,
  pub tag: String,
  /// The package's digest, as GitHub reported it.
  pub digest: Option<String>,
  /// Seconds since 1970.
  pub installed_at: u64,
}

impl Source {
  pub fn new(repository: Repo, tag: String, digest: Option<String>) -> Self {
    let installed_at = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_or(0, |elapsed| elapsed.as_secs());
    Self {
      repository,
      tag,
      digest,
      installed_at,
    }
  }
}

/// What a package's manifest says, to show before installing it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
  pub id: String,
  pub name: String,
  pub version: String,
  pub description: String,
  pub author: Option<String>,
  /// What each requested permission allows, in words.
  pub permissions: Vec<String>,
}

/// An extension in the extensions folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
  pub id: String,
  pub version: Version,
  pub dir: PathBuf,
  /// `None` if it was copied in by hand.
  pub source: Option<Source>,
}

/// A package unpacked and checked, ready to install. Dropping it removes
/// the unpacked files.
pub struct Staged {
  _dir: tempfile::TempDir,
  /// The folder with the manifest, in the staging folder.
  root: PathBuf,
  pub summary: Summary,
}

/// The extensions folder, and a staging folder on the same volume.
#[derive(Clone, Debug)]
pub struct Installer {
  pub extensions_dir: PathBuf,
  pub staging_dir: PathBuf,
  pub editor_version: Version,
}

impl Installer {
  /// A temporary file in the staging folder, to download a package into.
  pub fn download_file(&self) -> Result<tempfile::NamedTempFile, InstallError> {
    fs::create_dir_all(&self.staging_dir).map_err(io)?;
    tempfile::Builder::new()
      .prefix("download-")
      .suffix(".zip")
      .tempfile_in(&self.staging_dir)
      .map_err(io)
  }

  /// Unpacks a package and checks it.
  pub fn stage(&self, package: impl Read + Seek) -> Result<Staged, InstallError> {
    fs::create_dir_all(&self.staging_dir).map_err(io)?;
    let dir = tempfile::Builder::new()
      .prefix("package-")
      .tempdir_in(&self.staging_dir)
      .map_err(io)?;
    unpack(package, dir.path())?;
    let root = manifest_root(dir.path())?;
    let text = fs::read_to_string(root.join(MANIFEST_FILE))
      .map_err(|error| InstallError::InvalidManifest(error.to_string()))?;
    let manifest = manifest::parse(&text, &root, &self.editor_version, false)
      .map_err(|error| InstallError::InvalidManifest(error.to_string()))?;
    if let Some(entrypoint) = &manifest.entrypoint
      && !entrypoint.is_file()
    {
      return Err(InstallError::InvalidManifest(format!(
        "its entrypoint {} isn't in the package",
        entrypoint
          .strip_prefix(&root)
          .unwrap_or(entrypoint)
          .display()
      )));
    }
    let summary = Summary {
      id: manifest.id,
      name: manifest.name,
      version: manifest.version.to_string(),
      description: manifest.description,
      author: manifest.author,
      permissions: manifest
        .permissions
        .iter()
        .map(|permission| permission.description().to_owned())
        .collect(),
    };
    Ok(Staged {
      _dir: dir,
      root,
      summary,
    })
  }

  /// The installed extension with this id, if any.
  pub fn find(&self, id: &str) -> Option<Installed> {
    self
      .installed()
      .into_iter()
      .find(|installed| installed.id == id)
  }

  /// Installs a staged package, replacing the installed extension with its
  /// id, if any. Returns its folder.
  pub fn install(&self, staged: Staged, source: &Source) -> Result<PathBuf, InstallError> {
    let record = serde_json::to_vec_pretty(source).map_err(|e| InstallError::Io(e.to_string()))?;
    fs::write(staged.root.join(SOURCE_FILE), record).map_err(io)?;
    fs::create_dir_all(&self.extensions_dir).map_err(io)?;
    let target = self.find(&staged.summary.id).map_or_else(
      || self.extensions_dir.join(&staged.summary.id),
      |old| old.dir,
    );
    if !target.exists() {
      fs::rename(&staged.root, &target).map_err(io)?;
      return Ok(target);
    }
    // Move the old one aside, then the new one in; put the old one back if
    // that fails.
    let aside = tempfile::Builder::new()
      .prefix("replaced-")
      .tempdir_in(&self.staging_dir)
      .map_err(io)?;
    let old = aside.path().join("extension");
    fs::rename(&target, &old).map_err(|error| {
      InstallError::Io(format!(
        "{} can't be replaced (is a file in it open?): {error}",
        target.display()
      ))
    })?;
    if let Err(error) = fs::rename(&staged.root, &target) {
      let _ = fs::rename(&old, &target);
      return Err(io(error));
    }
    Ok(target)
  }

  /// Removes an installed extension's folder. It's moved out first, so it
  /// goes all at once or not at all.
  pub fn uninstall(&self, id: &str) -> Result<(), InstallError> {
    let installed = self
      .find(id)
      .ok_or_else(|| InstallError::NotFound(format!("an installed extension {id}")))?;
    fs::create_dir_all(&self.staging_dir).map_err(io)?;
    let aside = tempfile::Builder::new()
      .prefix("removed-")
      .tempdir_in(&self.staging_dir)
      .map_err(io)?;
    fs::rename(&installed.dir, aside.path().join("extension")).map_err(|error| {
      InstallError::Io(format!(
        "{} can't be removed (is a file in it open?): {error}",
        installed.dir.display()
      ))
    })
  }

  /// The extensions in the extensions folder, with where they came from.
  pub fn installed(&self) -> Vec<Installed> {
    let Ok(entries) = fs::read_dir(&self.extensions_dir) else {
      return Vec::new();
    };
    let mut installed: Vec<Installed> = entries
      .filter_map(Result::ok)
      .map(|entry| entry.path())
      .filter_map(|dir| {
        let text = fs::read_to_string(dir.join(MANIFEST_FILE)).ok()?;
        let raw: RawManifest = toml::from_str(&text).ok()?;
        let source = fs::read(dir.join(SOURCE_FILE))
          .ok()
          .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        Some(Installed {
          id: raw.id,
          version: Version::parse(&raw.version).ok()?,
          dir,
          source,
        })
      })
      .collect();
    installed.sort_by(|a, b| a.dir.cmp(&b.dir));
    installed
  }
}

#[derive(Deserialize)]
struct RawManifest {
  id: String,
  version: String,
}

fn io(error: std::io::Error) -> InstallError {
  InstallError::Io(error.to_string())
}

fn bad(message: impl Into<String>) -> InstallError {
  InstallError::InvalidPackage(message.into())
}

/// Unpacks a zip file into `dir`: only regular files and folders, inside
/// `dir`, within the limits.
fn unpack(package: impl Read + Seek, dir: &Path) -> Result<(), InstallError> {
  let mut archive =
    zip::ZipArchive::new(package).map_err(|error| bad(format!("it isn't a zip file: {error}")))?;
  if archive.len() > MAX_FILES {
    return Err(bad(format!("it has more than {MAX_FILES} files")));
  }
  let mut total: u64 = 0;
  for index in 0..archive.len() {
    let mut entry = archive
      .by_index(index)
      .map_err(|error| bad(error.to_string()))?;
    let name = entry
      .name()
      .map_or_else(|_| format!("entry {index}"), |name| name.into_owned());
    let Some(relative) = entry.enclosed_name() else {
      return Err(bad(format!("{name:?} would be outside its folder")));
    };
    if entry.is_symlink() {
      return Err(bad(format!("{name:?} is a link")));
    }
    let path = dir.join(relative);
    if entry.is_dir() {
      fs::create_dir_all(&path).map_err(io)?;
      continue;
    }
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).map_err(io)?;
    }
    let mut file = fs::File::create_new(&path).map_err(|error| {
      if error.kind() == std::io::ErrorKind::AlreadyExists {
        bad(format!("{name:?} is in it twice"))
      } else {
        io(error)
      }
    })?;
    // Count what's actually written, not what the entry claims.
    let remaining = MAX_UNPACKED_BYTES - total;
    let written = std::io::copy(&mut (&mut entry).take(remaining + 1), &mut file)
      .map_err(|error| bad(format!("{name:?} can't be unpacked: {error}")))?;
    total += written;
    if total > MAX_UNPACKED_BYTES {
      return Err(bad(format!(
        "it unpacks to more than {} MiB",
        MAX_UNPACKED_BYTES / 1024 / 1024
      )));
    }
  }
  Ok(())
}

/// The folder with the manifest: the package's root, or its one folder.
fn manifest_root(dir: &Path) -> Result<PathBuf, InstallError> {
  if dir.join(MANIFEST_FILE).is_file() {
    return Ok(dir.to_path_buf());
  }
  let entries: Vec<PathBuf> = fs::read_dir(dir)
    .map_err(io)?
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .collect();
  match entries.as_slice() {
    [only] if only.is_dir() && only.join(MANIFEST_FILE).is_file() => Ok(only.clone()),
    _ => Err(bad(format!("it has no {MANIFEST_FILE} at its root"))),
  }
}
