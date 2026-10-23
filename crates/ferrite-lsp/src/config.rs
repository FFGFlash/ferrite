//! Which language servers exist and where their workspaces are.

use std::path::{Path, PathBuf};

use crate::client::ServerCommand;

/// A language server and the languages it serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
  /// Identifies the server in settings and the UI.
  pub name: String,
  pub command: ServerCommand,
  /// LSP language identifiers, e.g. `rust`.
  pub languages: Vec<String>,
  /// Files marking a project root, nearest first, used when a document is
  /// outside the open workspace folder.
  pub root_markers: Vec<String>,
}

fn config(
  name: &str,
  program: &str,
  args: &[&str],
  languages: &[&str],
  markers: &[&str],
) -> ServerConfig {
  let strings = |items: &[&str]| items.iter().map(|&item| item.to_owned()).collect();
  ServerConfig {
    name: name.to_owned(),
    command: ServerCommand {
      program: program.to_owned(),
      args: strings(args),
    },
    languages: strings(languages),
    root_markers: strings(markers),
  }
}

/// Servers used when installed, unless settings say otherwise.
pub fn default_servers() -> Vec<ServerConfig> {
  vec![
    config(
      "rust-analyzer",
      "rust-analyzer",
      &[],
      &["rust"],
      &["Cargo.toml"],
    ),
    config(
      "typescript-language-server",
      "typescript-language-server",
      &["--stdio"],
      &["typescript", "typescriptreact", "javascript"],
      &["tsconfig.json", "jsconfig.json", "package.json"],
    ),
    config(
      "pyright",
      "pyright-langserver",
      &["--stdio"],
      &["python"],
      &["pyproject.toml", "setup.py", "pyrightconfig.json"],
    ),
  ]
}

/// The workspace root for a server handling `file`: the open workspace
/// folder if it contains the file, else the nearest ancestor with a root
/// marker, else the file's directory.
pub fn find_root(file: &Path, markers: &[String], workspace: Option<&Path>) -> PathBuf {
  if let Some(workspace) = workspace
    && file.starts_with(workspace)
  {
    return workspace.to_path_buf();
  }
  let directory = file.parent().unwrap_or(file);
  directory
    .ancestors()
    .find(|dir| markers.iter().any(|marker| dir.join(marker).exists()))
    .unwrap_or(directory)
    .to_path_buf()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn roots_prefer_the_workspace_then_markers() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project = temp.path().join("project");
    let source = project.join("src");
    std::fs::create_dir_all(&source).expect("creates");
    std::fs::write(project.join("Cargo.toml"), "").expect("writes");
    let file = source.join("main.rs");
    let markers = vec!["Cargo.toml".to_owned()];

    assert_eq!(find_root(&file, &markers, Some(temp.path())), temp.path());
    assert_eq!(find_root(&file, &markers, None), project);
    assert_eq!(find_root(&file, &[], None), source);
    let elsewhere = tempfile::tempdir().expect("temp dir");
    assert_eq!(find_root(&file, &markers, Some(elsewhere.path())), project);
  }

  #[test]
  fn defaults_cover_rust() {
    let servers = default_servers();
    assert!(
      servers
        .iter()
        .any(|server| server.languages.contains(&"rust".to_owned()))
    );
  }
}
