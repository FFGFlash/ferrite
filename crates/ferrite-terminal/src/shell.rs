//! Which shell a new terminal runs.

use std::path::{Path, PathBuf};

/// A program to run in a terminal, with its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
  pub program: String,
  pub args: Vec<String>,
}

impl Shell {
  /// The platform's usual interactive shell:
  ///
  /// - Windows: PowerShell 7 (`pwsh`) if it's on the `PATH`, otherwise
  ///   Windows PowerShell, which every installation has.
  /// - macOS: `$SHELL` as a login shell, as Terminal.app runs it, so
  ///   profile scripts set up the `PATH` for apps started from the Dock.
  /// - Others: `$SHELL`, or `/bin/sh`.
  pub fn detect() -> Self {
    #[cfg(windows)]
    {
      let program = find_on_path("pwsh.exe")
        .or_else(|| {
          let root = std::env::var_os("SystemRoot")?;
          let path = Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
          path.is_file().then_some(path)
        })
        .map_or_else(
          || "powershell.exe".to_owned(),
          |path| path.display().to_string(),
        );
      Self {
        program,
        args: Vec::new(),
      }
    }
    #[cfg(not(windows))]
    {
      let program = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| "/bin/sh".to_owned());
      let args = if cfg!(target_os = "macos") {
        vec!["-l".to_owned()]
      } else {
        Vec::new()
      };
      Self { program, args }
    }
  }

  /// A short name for the shell, such as `pwsh` or `bash`.
  pub fn name(&self) -> String {
    Path::new(&self.program).file_stem().map_or_else(
      || self.program.clone(),
      |stem| stem.to_string_lossy().into_owned(),
    )
  }
}

/// The first file named `name` in a `PATH` directory.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
  let path = std::env::var_os("PATH")?;
  std::env::split_paths(&path)
    .map(|dir| dir.join(name))
    .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn detects_a_shell_that_exists() {
    let shell = Shell::detect();
    let program = Path::new(&shell.program);
    assert!(
      program.is_file() || find_on_path(&shell.program).is_some(),
      "{shell:?}"
    );
  }

  #[test]
  fn names_shells_by_their_file_stem() {
    let shell = |program: &str| Shell {
      program: program.into(),
      args: Vec::new(),
    };
    #[cfg(windows)]
    assert_eq!(
      shell(r"C:\Program Files\PowerShell\7\pwsh.exe").name(),
      "pwsh"
    );
    assert_eq!(shell("/bin/bash").name(), "bash");
    assert_eq!(shell("zsh").name(), "zsh");
  }
}
