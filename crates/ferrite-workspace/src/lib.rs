//! Filesystem access for Ferrite.
//!
//! This crate reads and writes files; it holds no editor state. In later
//! milestones it also owns workspace roots, file discovery, and change
//! notifications.

mod file;

pub use file::{FileError, FileStamp, LoadedFile, MAX_FILE_BYTES, load_text_file, save_text_file};
