//! A program a terminal runs. Which ones are installed is up to
//! [`profiles`](crate::profiles).

use std::path::PathBuf;

/// A program to run in a terminal, with its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
  pub program: String,
  pub args: Vec<String>,
}

/// The first file named `name` in a `PATH` directory.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
  let path = std::env::var_os("PATH")?;
  std::env::split_paths(&path)
    .map(|dir| dir.join(name))
    .find(|candidate| candidate.is_file())
}
