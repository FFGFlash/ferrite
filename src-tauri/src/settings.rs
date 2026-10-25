//! User settings, stored as TOML in the platform config directory and edited
//! in Ferrite itself. Saving the file applies it; invalid files are reported
//! and the previous settings stay in effect.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use ferrite_core::EditorConfig;
use ferrite_lsp::{ServerCommand, ServerConfig, default_servers};
use ferrite_workspace::ExcludeFilter;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::editor::EditorState;
use crate::error::CommandError;
use crate::events::{self, AppEvent};

const FILE_NAME: &str = "settings.toml";

/// Written when the settings file does not exist yet. Every setting is shown
/// with its default value, commented out.
const TEMPLATE: &str = r#"# Ferrite settings. Uncomment a line to change it; save to apply.

[editor]
# tab_size = 4
# insert_spaces = true
# font_size = 14
# font_family = ""          # empty: the default monospace font

[workbench]
# theme = "system"          # "system", "dark", or "light"

[files]
# Hidden from the explorer, Go to File, and search. Patterns without "/"
# match names at any depth; others match paths relative to the folder.
# exclude = [".git", ".DS_Store", "Thumbs.db"]

[search]
# max_file_bytes = 4194304  # larger files are skipped by find in files

[lsp]
# Language servers provide diagnostics, hover, go to definition, and
# completion. Known servers start when a file needs one, if installed:
# rust-analyzer, typescript-language-server, and pyright-langserver.
# enabled = true
#
# Change a server, or add one (languages: rust, typescript, typescriptreact,
# javascript, python, json, toml):
# [lsp.servers.rust-analyzer]
# command = "rust-analyzer"
# args = []
# enabled = true
#
# [lsp.servers.my-server]
# command = "my-language-server"
# args = ["--stdio"]
# languages = ["python"]
# root_markers = ["pyproject.toml"]

[extensions]
# WebAssembly extensions. Installed extensions are folders in "extensions"
# next to this file. Each asks before using the permissions it requests.
# enabled = true
#
# Folders of extensions under development, loaded in place:
# development = ["C:/code/my-extension"]
#
# Settings that extensions declare, by extension id:
# [extensions.settings."example.tidy"]
# max_line_length = 120
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
  #[default]
  System,
  Dark,
  Light,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
  pub editor: EditorSettings,
  pub workbench: WorkbenchSettings,
  pub files: FilesSettings,
  pub search: SearchSettings,
  pub lsp: LspSettings,
  pub extensions: ExtensionsSettings,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExtensionsSettings {
  pub enabled: bool,
  /// Folders of development extensions.
  pub development: Vec<String>,
  /// Values for the settings extensions declare, by extension id.
  pub settings: BTreeMap<String, toml::Table>,
}

impl Default for ExtensionsSettings {
  fn default() -> Self {
    Self {
      enabled: true,
      development: Vec::new(),
      settings: BTreeMap::new(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorSettings {
  pub tab_size: usize,
  pub insert_spaces: bool,
  pub font_size: u32,
  pub font_family: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WorkbenchSettings {
  pub theme: Theme,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FilesSettings {
  pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SearchSettings {
  pub max_file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LspSettings {
  pub enabled: bool,
  pub servers: BTreeMap<String, ServerSettings>,
}

impl Default for LspSettings {
  fn default() -> Self {
    Self {
      enabled: true,
      servers: BTreeMap::new(),
    }
  }
}

/// Changes to a known server, or a new one (which needs `command` and
/// `languages`).
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ServerSettings {
  pub enabled: Option<bool>,
  pub command: Option<String>,
  pub args: Option<Vec<String>>,
  pub languages: Option<Vec<String>>,
  pub root_markers: Option<Vec<String>>,
}

impl Default for EditorSettings {
  fn default() -> Self {
    Self {
      tab_size: 4,
      insert_spaces: true,
      font_size: 14,
      font_family: String::new(),
    }
  }
}

impl Default for FilesSettings {
  fn default() -> Self {
    Self {
      exclude: vec![".git".into(), ".DS_Store".into(), "Thumbs.db".into()],
    }
  }
}

impl Default for SearchSettings {
  fn default() -> Self {
    Self {
      max_file_bytes: 4 * 1024 * 1024,
    }
  }
}

/// Settings the frontend applies itself.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendSettings {
  pub theme: Theme,
  pub font_size: u32,
  pub font_family: String,
  pub tab_size: usize,
}

impl Settings {
  /// Parses and validates a settings file.
  pub fn parse(text: &str) -> Result<Self, String> {
    let settings: Self = toml::from_str(text).map_err(|error| error.to_string())?;
    settings.validate()?;
    Ok(settings)
  }

  fn validate(&self) -> Result<(), String> {
    if !(1..=16).contains(&self.editor.tab_size) {
      return Err("editor.tab_size must be between 1 and 16".into());
    }
    if !(6..=72).contains(&self.editor.font_size) {
      return Err("editor.font_size must be between 6 and 72".into());
    }
    if self.search.max_file_bytes == 0 {
      return Err("search.max_file_bytes must be greater than 0".into());
    }
    ExcludeFilter::new(&self.files.exclude).map_err(|error| format!("files.exclude: {error}"))?;
    let known: Vec<String> = default_servers()
      .into_iter()
      .map(|server| server.name)
      .collect();
    for (name, server) in &self.lsp.servers {
      if !known.contains(name) && (server.command.is_none() || server.languages.is_none()) {
        return Err(format!(
          "lsp.servers.{name} needs a command and languages (known servers: {})",
          known.join(", ")
        ));
      }
    }
    if self
      .extensions
      .development
      .iter()
      .any(|folder| folder.trim().is_empty())
    {
      return Err("extensions.development entries must be folder paths".into());
    }
    Ok(())
  }

  /// The language servers to use: the known ones with any changes from
  /// settings, plus added ones, minus disabled ones.
  pub fn language_servers(&self) -> Vec<ServerConfig> {
    if !self.lsp.enabled {
      return Vec::new();
    }
    let mut servers = default_servers();
    for (name, settings) in &self.lsp.servers {
      let index = match servers.iter().position(|server| server.name == *name) {
        Some(index) => index,
        None => {
          servers.push(ServerConfig {
            name: name.clone(),
            command: ServerCommand {
              program: String::new(),
              args: Vec::new(),
            },
            languages: Vec::new(),
            root_markers: Vec::new(),
          });
          servers.len() - 1
        }
      };
      let server = &mut servers[index];
      if let Some(command) = &settings.command {
        server.command.program = command.clone();
      }
      if let Some(args) = &settings.args {
        server.command.args = args.clone();
      }
      if let Some(languages) = &settings.languages {
        server.languages = languages.clone();
      }
      if let Some(markers) = &settings.root_markers {
        server.root_markers = markers.clone();
      }
    }
    servers
      .retain(|server| self.lsp.servers.get(&server.name).and_then(|s| s.enabled) != Some(false));
    servers
  }

  pub fn exclude_filter(&self) -> ExcludeFilter {
    // Validated on load, so building cannot fail; fall back to no exclusions.
    ExcludeFilter::new(&self.files.exclude).unwrap_or_default()
  }

  pub fn editor_config(&self) -> EditorConfig {
    EditorConfig {
      tab_size: self.editor.tab_size,
      insert_spaces: self.editor.insert_spaces,
      ..EditorConfig::default()
    }
  }

  pub fn frontend(&self) -> FrontendSettings {
    FrontendSettings {
      theme: self.workbench.theme,
      font_size: self.editor.font_size,
      font_family: self.editor.font_family.clone(),
      tab_size: self.editor.tab_size,
    }
  }
}

pub struct SettingsState {
  path: Option<PathBuf>,
  current: RwLock<Settings>,
}

impl SettingsState {
  /// Loads settings from the config directory. Problems are logged and the
  /// defaults are used.
  pub fn load(config_dir: Option<PathBuf>) -> Self {
    let path = config_dir.map(|dir| dir.join(FILE_NAME));
    let settings = match path.as_deref().map(std::fs::read_to_string) {
      Some(Ok(text)) => Settings::parse(&text).unwrap_or_else(|error| {
        tracing::warn!(%error, "invalid settings file; using defaults");
        Settings::default()
      }),
      Some(Err(error)) if error.kind() != std::io::ErrorKind::NotFound => {
        tracing::warn!(%error, "could not read settings; using defaults");
        Settings::default()
      }
      _ => Settings::default(),
    };
    Self {
      path,
      current: RwLock::new(settings),
    }
  }

  pub fn get(&self) -> Settings {
    self
      .current
      .read()
      .unwrap_or_else(PoisonError::into_inner)
      .clone()
  }

  pub fn path(&self) -> Option<&Path> {
    self.path.as_deref()
  }

  fn replace(&self, settings: Settings) {
    *self.current.write().unwrap_or_else(PoisonError::into_inner) = settings;
  }
}

/// Re-reads the settings file after it was saved and applies it everywhere.
/// Returns an error message for the user if the file is invalid.
pub fn reload(app: &AppHandle) -> Result<(), CommandError> {
  let state = app.state::<SettingsState>();
  let Some(path) = state.path() else {
    return Ok(());
  };
  let text = std::fs::read_to_string(path)
    .map_err(|error| CommandError::new("settings", format!("Could not read settings: {error}")))?;
  let settings = Settings::parse(&text).map_err(|error| {
    CommandError::new("invalidSettings", format!("Settings not applied: {error}"))
  })?;
  let previous = state.get();
  state.replace(settings.clone());
  apply(app, &previous, &settings);
  tracing::info!("settings applied");
  Ok(())
}

fn apply(app: &AppHandle, previous: &Settings, settings: &Settings) {
  app
    .state::<EditorState>()
    .lock()
    .set_config(settings.editor_config());
  if previous.files.exclude != settings.files.exclude {
    crate::workspace::exclusions_changed(app);
  }
  if previous.lsp != settings.lsp {
    app.state::<crate::lsp::LspState>().restart_all(app);
  }
  crate::extensions::settings_changed(app, previous, settings);
  events::emit(
    app,
    AppEvent::SettingsChanged {
      settings: settings.frontend(),
    },
  );
}

/// Whether `path` is the settings file.
pub fn is_settings_file(app: &AppHandle, path: &Path) -> bool {
  app
    .state::<SettingsState>()
    .path()
    .is_some_and(|settings| crate::paths::normalize(settings) == path)
}

#[tauri::command]
pub fn settings_get(state: State<'_, SettingsState>) -> FrontendSettings {
  state.get().frontend()
}

/// The settings file's path, creating it from the template if needed.
#[tauri::command]
pub fn settings_file(state: State<'_, SettingsState>) -> Result<String, CommandError> {
  let path = state
    .path()
    .ok_or_else(|| CommandError::new("settings", "No configuration directory is available."))?;
  if !path.exists() {
    if let Some(dir) = path.parent() {
      std::fs::create_dir_all(dir).map_err(|error| {
        CommandError::new(
          "settings",
          format!("Could not create {}: {error}", dir.display()),
        )
      })?;
    }
    std::fs::write(path, TEMPLATE).map_err(|error| {
      CommandError::new("settings", format!("Could not create settings: {error}"))
    })?;
  }
  Ok(path.display().to_string())
}

/// Sets `workbench.theme` in the settings file, keeping its comments and
/// formatting, and applies it.
#[tauri::command]
pub fn settings_set_theme(app: AppHandle, theme: Theme) -> Result<(), CommandError> {
  let name = match theme {
    Theme::System => "system",
    Theme::Dark => "dark",
    Theme::Light => "light",
  };
  edit(&app, |document| {
    if !document.contains_table("workbench") {
      document["workbench"] = toml_edit::table();
    }
    document["workbench"]["theme"] = toml_edit::value(name);
    Ok(())
  })
}

/// Changes the settings file with `change`, keeping its comments and
/// formatting, and applies the result.
pub fn edit(
  app: &AppHandle,
  change: impl FnOnce(&mut toml_edit::DocumentMut) -> Result<(), String>,
) -> Result<(), CommandError> {
  let path = PathBuf::from(settings_file(app.state())?);
  let text = std::fs::read_to_string(&path)
    .map_err(|error| CommandError::new("settings", format!("Could not read settings: {error}")))?;
  let mut document: toml_edit::DocumentMut = text.parse().map_err(|error| {
    CommandError::new(
      "invalidSettings",
      format!("Settings file is invalid: {error}"),
    )
  })?;
  change(&mut document).map_err(|message| CommandError::new("invalidSettings", message))?;
  std::fs::write(&path, document.to_string())
    .map_err(|error| CommandError::new("settings", format!("Could not write settings: {error}")))?;
  reload(app)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_template_parses_to_the_defaults() {
    assert_eq!(Settings::parse(TEMPLATE), Ok(Settings::default()));
  }

  #[test]
  fn parses_partial_settings_over_defaults() {
    let settings =
      Settings::parse("[editor]\ntab_size = 2\n[workbench]\ntheme = \"light\"\n").expect("valid");
    assert_eq!(settings.editor.tab_size, 2);
    assert!(settings.editor.insert_spaces);
    assert_eq!(settings.workbench.theme, Theme::Light);
    assert_eq!(settings.files.exclude, FilesSettings::default().exclude);
  }

  #[test]
  fn rejects_invalid_settings_with_a_message() {
    let unknown = Settings::parse("[editor]\ntab_width = 2\n").expect_err("unknown key");
    assert!(unknown.contains("tab_width"), "{unknown}");
    let range = Settings::parse("[editor]\ntab_size = 0\n").expect_err("out of range");
    assert!(range.contains("tab_size"));
    let theme = Settings::parse("[workbench]\ntheme = \"neon\"\n").expect_err("bad theme");
    assert!(theme.contains("neon"), "{theme}");
    let glob = Settings::parse("[files]\nexclude = [\"[x\"]\n").expect_err("bad glob");
    assert!(glob.contains("files.exclude"));
    let server =
      Settings::parse("[lsp.servers.mine]\ncommand = \"x\"\n").expect_err("no languages");
    assert!(server.contains("lsp.servers.mine"), "{server}");
  }

  #[test]
  fn parses_extension_settings() {
    let settings = Settings::parse(
      r#"
[extensions]
development = ["C:/code/tidy"]
[extensions.settings."example.tidy"]
max_line_length = 80
"#,
    )
    .expect("valid");
    assert!(settings.extensions.enabled);
    assert_eq!(settings.extensions.development, ["C:/code/tidy"]);
    assert_eq!(
      settings.extensions.settings["example.tidy"]["max_line_length"].as_integer(),
      Some(80)
    );
    let empty = Settings::parse("[extensions]\ndevelopment = [\" \"]\n").expect_err("empty path");
    assert!(empty.contains("extensions.development"), "{empty}");
  }

  #[test]
  fn language_servers_merge_settings_over_the_known_ones() {
    let settings = Settings::parse(
      r#"
[lsp.servers.rust-analyzer]
command = "/opt/ra"
[lsp.servers.pyright]
enabled = false
[lsp.servers.mine]
command = "mine"
args = ["--stdio"]
languages = ["toml"]
"#,
    )
    .expect("valid");
    let servers = settings.language_servers();
    let rust = servers
      .iter()
      .find(|s| s.name == "rust-analyzer")
      .expect("rust");
    assert_eq!(rust.command.program, "/opt/ra");
    assert_eq!(rust.languages, ["rust"]);
    assert!(servers.iter().all(|s| s.name != "pyright"));
    let mine = servers.iter().find(|s| s.name == "mine").expect("added");
    assert_eq!(mine.command.args, ["--stdio"]);
    let disabled = Settings::parse("[lsp]\nenabled = false\n").expect("valid");
    assert!(disabled.language_servers().is_empty());
  }
}
