//! The host functions of the `workspace` interface (API 1.6): the open
//! folder and whether it's trusted, and watching files for changes.
//!
//! Watching a path inside the open folder needs `files-read`, and uses the
//! changes the editor already watches the folder for. A path outside it
//! (such as the `.git` folder of a repository the open folder is inside)
//! needs `processes`, whose programs can read it anyway, and gets a watcher
//! of its own.

use std::path::Path;

use ferrite_workspace::normalize_path;

use crate::Notification;
use crate::bindings::ferrite::extension::files::FileError;
use crate::bindings::ferrite::extension::workspace::{FileChanges, FolderInfo, Host};
use crate::permissions::Permission;
use crate::runtime::StoreState;
use crate::worker::Payload;

impl Host for StoreState {
  fn folder(&mut self) -> Option<FolderInfo> {
    let folder = self.api.shared().ok()?.embedder.open_folder()?;
    Some(FolderInfo {
      path: folder.root.display().to_string(),
      trusted: folder.trusted,
    })
  }

  fn request_trust(&mut self) {
    if let Ok(shared) = self.api.shared()
      && shared
        .embedder
        .open_folder()
        .is_some_and(|folder| !folder.trusted)
    {
      shared.embedder.notify(Notification::TrustRequested {
        extension: self.api.extension_id().to_owned(),
      });
    }
  }

  fn open_file(&mut self, path: String) -> Result<(), FileError> {
    let given = Path::new(&path);
    if !given.is_absolute() {
      return Err(FileError::Failed(format!(
        "{path:?} is not an absolute path"
      )));
    }
    if given.is_dir() {
      return Err(FileError::WrongKind);
    }
    if !given.is_file() {
      return Err(FileError::NotFound);
    }
    let shared = self
      .api
      .shared()
      .map_err(|_| FileError::Failed("the editor is closing".into()))?;
    shared.embedder.notify(Notification::OpenFile {
      path: normalize_path(given),
    });
    Ok(())
  }

  fn open_folder(&mut self, path: String) -> Result<(), FileError> {
    let given = Path::new(&path);
    if !given.is_absolute() {
      return Err(FileError::Failed(format!(
        "{path:?} is not an absolute path"
      )));
    }
    if given.is_file() {
      return Err(FileError::WrongKind);
    }
    if !given.is_dir() {
      return Err(FileError::NotFound);
    }
    let shared = self
      .api
      .shared()
      .map_err(|_| FileError::Failed("the editor is closing".into()))?;
    shared.embedder.notify(Notification::OpenFolder {
      path: normalize_path(given),
    });
    Ok(())
  }

  fn watch(&mut self, path: String, command: String) -> Result<u64, FileError> {
    let api = &self.api;
    if !api.manifest().declares_command(&command) {
      return Err(FileError::Failed(format!(
        "{command:?} is not one of this extension's commands"
      )));
    }
    let shared = api
      .shared()
      .map_err(|_| FileError::Failed("the editor is closing".into()))?;
    let given = Path::new(&path);
    if !given.is_absolute() {
      return Err(FileError::Failed(format!(
        "{path:?} is not an absolute path"
      )));
    }
    if !given.exists() {
      return Err(FileError::NotFound);
    }
    let resolved = normalize_path(given);
    let inside = shared
      .embedder
      .open_folder()
      .is_some_and(|folder| resolved.starts_with(&folder.root));
    let (needed, name) = if inside {
      (Permission::FilesRead, "files-read")
    } else {
      (Permission::Processes, "processes")
    };
    if !api.has(needed) {
      return Err(FileError::PermissionDenied(name.into()));
    }
    shared
      .add_watch(
        api.extension_id(),
        api.generation(),
        resolved,
        !inside,
        command,
      )
      .map_err(|message| {
        if message.starts_with("at most") {
          FileError::TooLarge(message)
        } else {
          FileError::Failed(message)
        }
      })
  }

  fn unwatch(&mut self, watch: u64) {
    if let Ok(shared) = self.api.shared() {
      shared.remove_watch(self.api.extension_id(), watch);
    }
  }

  fn changes(&mut self) -> Option<FileChanges> {
    match &self.api.payload {
      Payload::Changes(changes) => Some(changes.clone()),
      _ => None,
    }
  }
}
