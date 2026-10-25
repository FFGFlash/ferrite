//! Filesystem access for Ferrite: loading and saving files, listing
//! directories for the explorer, the Go to File index, find in files,
//! change notifications, and backups of unsaved documents. This crate holds
//! no editor state.

mod backup;
mod file;
mod filters;
mod index;
mod search;
mod tree;
mod watcher;

pub use backup::{Abandoned, BackupMeta, BackupSession, RecoveredBackup};
pub use file::{
  FileError, FileStamp, LoadedFile, MAX_FILE_BYTES, load_text_file, save_text_file,
  write_file_atomically,
};
pub use filters::{ExcludeFilter, FilterError, to_slash};
pub use index::{FileIndex, FileMatch, MAX_INDEXED_FILES, scan_files};
pub use search::{FileMatches, LineMatch, SearchLimits, SearchSummary, search_workspace};
pub use tree::{DirEntry, EntryKind, list_directory};
pub use watcher::{WatchError, WorkspaceWatcher};
