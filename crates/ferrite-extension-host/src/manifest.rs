//! Extension manifests (`extension.toml`): parsing and validation.
//!
//! A manifest is validated completely before anything is loaded: unknown
//! fields, unsupported API versions, editor version requirements that
//! aren't met, malformed identifiers, undeclared commands in activation
//! events, and configuration defaults of the wrong type are all rejected
//! with a message naming the problem.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::config::{ConfigProperty, ConfigType, ConfigValue};
use crate::contributions::{self, Contributions, RawContributions};
use crate::permissions::Permission;

/// The manifest file in an extension's folder.
pub const MANIFEST_FILE: &str = "extension.toml";
/// The extension API versions this host implements.
pub const SUPPORTED_API_VERSIONS: &[u32] = &[1];
/// Larger entry points are rejected.
pub const MAX_ENTRYPOINT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
  #[error("could not read {path}: {source}")]
  Read {
    path: PathBuf,
    source: std::io::Error,
  },
  #[error("{0}")]
  Invalid(String),
  #[error("requires extension API version {found}; this version of Ferrite supports {supported}")]
  UnsupportedApi { found: String, supported: String },
  #[error("requires Ferrite {required}; this is Ferrite {editor}")]
  IncompatibleEditor { required: String, editor: String },
}

fn invalid(message: impl Into<String>) -> ManifestError {
  ManifestError::Invalid(message.into())
}

/// When an extension is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activation {
  /// When the editor starts (or the extension is loaded).
  Startup,
  /// When one of its commands runs. Every extension activates for its own
  /// commands, so this is implied.
  Command(String),
  /// When a document of this language (e.g. `rust`) is opened.
  Language(String),
  /// When one of its side bar views is shown.
  View(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandContribution {
  pub id: String,
  pub title: String,
  /// Shown before the title in the command palette; the extension's name
  /// by default.
  pub category: String,
  /// Left out of the command palette (e.g. run only from view items).
  pub hidden: bool,
}

#[derive(Debug, Clone)]
pub struct Manifest {
  pub id: String,
  pub name: String,
  pub version: Version,
  pub description: String,
  pub api_version: u32,
  /// The folder the manifest is in.
  pub dir: PathBuf,
  /// The component or module to load, resolved against `dir`. Extensions
  /// that only contribute themes and icon themes have none.
  pub entrypoint: Option<PathBuf>,
  pub activation: Vec<Activation>,
  pub permissions: Vec<Permission>,
  pub commands: Vec<CommandContribution>,
  pub configuration: BTreeMap<String, ConfigProperty>,
  pub contributions: Contributions,
}

impl Manifest {
  pub fn activates_on_startup(&self) -> bool {
    self.activation.contains(&Activation::Startup)
  }

  pub fn activates_for_language(&self, language: &str) -> bool {
    self
      .activation
      .iter()
      .any(|activation| matches!(activation, Activation::Language(l) if l == language))
  }

  pub fn activates_for_view(&self, view: &str) -> bool {
    self
      .activation
      .iter()
      .any(|activation| matches!(activation, Activation::View(v) if v == view))
  }

  pub fn declares_command(&self, id: &str) -> bool {
    self.commands.iter().any(|command| command.id == id)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
  id: String,
  name: String,
  version: String,
  #[serde(default)]
  description: String,
  api_version: String,
  entrypoint: Option<String>,
  #[serde(default)]
  activation: Vec<String>,
  #[serde(default)]
  engines: RawEngines,
  #[serde(default)]
  permissions: RawPermissions,
  #[serde(default)]
  commands: Vec<RawCommand>,
  #[serde(default)]
  configuration: BTreeMap<String, RawProperty>,
  #[serde(default)]
  themes: Vec<contributions::RawTheme>,
  #[serde(default)]
  icon_themes: Vec<contributions::RawIconTheme>,
  #[serde(default)]
  keybindings: Vec<contributions::RawKeybinding>,
  #[serde(default)]
  menus: Vec<contributions::RawMenu>,
  #[serde(default)]
  status_items: Vec<contributions::RawStatusItem>,
  #[serde(default)]
  views: Vec<contributions::RawView>,
  #[serde(default)]
  decorations: BTreeMap<String, contributions::RawDecoration>,
  #[serde(default)]
  previews: Vec<contributions::RawPreview>,
  #[serde(default)]
  languages: Vec<contributions::RawLanguage>,
  #[serde(default)]
  language_servers: Vec<contributions::RawLanguageServer>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawEngines {
  ferrite: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawPermissions {
  #[serde(default)]
  documents_read: bool,
  #[serde(default)]
  documents_write: bool,
  #[serde(default)]
  language_servers: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommand {
  id: String,
  title: String,
  category: Option<String>,
  #[serde(default)]
  hidden: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProperty {
  #[serde(rename = "type")]
  kind: String,
  default: toml::Value,
  #[serde(default)]
  description: String,
  minimum: Option<f64>,
  maximum: Option<f64>,
  choices: Option<Vec<String>>,
}

/// Reads and validates `dir/extension.toml`. `development` extensions may
/// load an entry point from outside their folder (e.g. a build output).
pub fn load(
  dir: &Path,
  editor_version: &Version,
  development: bool,
) -> Result<Manifest, ManifestError> {
  let path = dir.join(MANIFEST_FILE);
  let read = |source| ManifestError::Read {
    path: path.clone(),
    source,
  };
  let size = std::fs::metadata(&path).map_err(read)?.len();
  if size > MAX_MANIFEST_BYTES {
    return Err(invalid(format!("{MANIFEST_FILE} is too large")));
  }
  let text = std::fs::read_to_string(&path).map_err(read)?;
  let mut manifest = parse(&text, dir, editor_version, development)?;
  if let Some(entrypoint) = &manifest.entrypoint {
    let entry = std::fs::metadata(entrypoint).map_err(|source| ManifestError::Read {
      path: entrypoint.clone(),
      source,
    })?;
    if !entry.is_file() {
      return Err(invalid(format!(
        "entrypoint {} is not a file",
        entrypoint.display()
      )));
    }
    if entry.len() > MAX_ENTRYPOINT_BYTES {
      return Err(invalid(format!(
        "entrypoint is larger than {} MiB",
        MAX_ENTRYPOINT_BYTES / 1024 / 1024
      )));
    }
  }
  for theme in &mut manifest.contributions.themes {
    contributions::load_theme_colors(dir, theme)?;
  }
  for theme in &mut manifest.contributions.icon_themes {
    contributions::load_icon_theme(dir, theme)?;
  }
  for language in &manifest.contributions.languages {
    contributions::check_language_files(dir, language)?;
  }
  Ok(manifest)
}

/// Parses and validates manifest text. Does not touch the filesystem.
pub fn parse(
  text: &str,
  dir: &Path,
  editor_version: &Version,
  development: bool,
) -> Result<Manifest, ManifestError> {
  let raw: RawManifest = toml::from_str(text).map_err(|error| invalid(error.to_string()))?;

  validate_id(&raw.id)?;
  let name = raw.name.trim();
  if name.is_empty() || name.chars().count() > 64 {
    return Err(invalid("name must be 1 to 64 characters"));
  }
  let version = Version::parse(&raw.version).map_err(|error| {
    invalid(format!(
      "version {:?} is not a semantic version: {error}",
      raw.version
    ))
  })?;
  if raw.description.chars().count() > 500 {
    return Err(invalid("description must be at most 500 characters"));
  }

  let api_version = raw
    .api_version
    .parse::<u32>()
    .ok()
    .filter(|version| SUPPORTED_API_VERSIONS.contains(version))
    .ok_or_else(|| ManifestError::UnsupportedApi {
      found: raw.api_version.clone(),
      supported: SUPPORTED_API_VERSIONS
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", "),
    })?;
  if let Some(required) = &raw.engines.ferrite {
    let requirement = VersionReq::parse(required)
      .map_err(|error| invalid(format!("engines.ferrite {required:?} is invalid: {error}")))?;
    if !requirement.matches(editor_version) {
      return Err(ManifestError::IncompatibleEditor {
        required: required.clone(),
        editor: editor_version.to_string(),
      });
    }
  }

  let entrypoint = raw
    .entrypoint
    .as_deref()
    .map(|entrypoint| resolve_entrypoint(dir, entrypoint, development))
    .transpose()?;

  let mut commands: Vec<CommandContribution> = Vec::new();
  for command in raw.commands {
    let prefix = format!("{}.", raw.id);
    let rest = command.id.strip_prefix(&prefix).unwrap_or_default();
    if rest.is_empty()
      || command.id.len() > 128
      || !rest
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
      return Err(invalid(format!(
        "command id {:?} must start with \"{prefix}\" followed by letters, digits, '.', '-', or '_'",
        command.id
      )));
    }
    if commands.iter().any(|existing| existing.id == command.id) {
      return Err(invalid(format!(
        "command {:?} is declared twice",
        command.id
      )));
    }
    let title = command.title.trim();
    if title.is_empty() || title.chars().count() > 100 {
      return Err(invalid(format!(
        "command {:?} needs a title of 1 to 100 characters",
        command.id
      )));
    }
    commands.push(CommandContribution {
      id: command.id,
      title: title.to_owned(),
      category: command
        .category
        .map_or_else(|| name.to_owned(), |category| category.trim().to_owned()),
      hidden: command.hidden,
    });
  }

  let mut activation = Vec::new();
  for event in &raw.activation {
    let parsed = if event == "on-startup" {
      Activation::Startup
    } else if let Some(command) = event.strip_prefix("on-command:") {
      if !commands.iter().any(|c| c.id == command) {
        return Err(invalid(format!(
          "activation {event:?} names a command that is not declared"
        )));
      }
      Activation::Command(command.to_owned())
    } else if let Some(language) = event.strip_prefix("on-language:")
      && !language.is_empty()
    {
      Activation::Language(language.to_owned())
    } else if let Some(view) = event.strip_prefix("on-view:") {
      if !raw.views.iter().any(|v| v.id == view) {
        return Err(invalid(format!(
          "activation {event:?} names a view that is not declared"
        )));
      }
      Activation::View(view.to_owned())
    } else {
      return Err(invalid(format!(
        "unknown activation event {event:?} (expected \"on-startup\", \"on-command:<id>\", \"on-language:<id>\", or \"on-view:<id>\")"
      )));
    };
    activation.push(parsed);
  }

  let mut permissions = Vec::new();
  if raw.permissions.documents_read {
    permissions.push(Permission::DocumentsRead);
  }
  if raw.permissions.documents_write {
    permissions.push(Permission::DocumentsWrite);
  }
  if raw.permissions.language_servers {
    permissions.push(Permission::LanguageServers);
  }
  if raw.permissions.language_servers == raw.language_servers.is_empty() {
    return Err(invalid(
      "permissions.language_servers must be requested exactly when language_servers are declared",
    ));
  }

  let mut configuration = BTreeMap::new();
  for (key, property) in raw.configuration {
    if key.is_empty()
      || key.len() > 64
      || !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
      return Err(invalid(format!(
        "configuration key {key:?} must be letters, digits, '_', '-', or '.'"
      )));
    }
    let property = parse_property(&key, property)?;
    configuration.insert(key, property);
  }

  let contributions = contributions::parse(
    RawContributions {
      themes: raw.themes,
      icon_themes: raw.icon_themes,
      keybindings: raw.keybindings,
      menus: raw.menus,
      status_items: raw.status_items,
      views: raw.views,
      decorations: raw.decorations,
      previews: raw.previews,
      languages: raw.languages,
      language_servers: raw.language_servers,
    },
    &raw.id,
    &commands,
  )?;
  // The editor starts language servers itself.
  let code_permissions = permissions
    .iter()
    .any(|&permission| permission != Permission::LanguageServers);
  if entrypoint.is_none()
    && (!commands.is_empty()
      || !activation.is_empty()
      || code_permissions
      || !configuration.is_empty()
      || contributions.needs_code())
  {
    return Err(invalid(
      "entrypoint is required unless the extension only contributes themes, icon themes, languages, and language servers",
    ));
  }

  Ok(Manifest {
    id: raw.id,
    name: name.to_owned(),
    version,
    description: raw.description.trim().to_owned(),
    api_version,
    dir: dir.to_path_buf(),
    entrypoint,
    activation,
    permissions,
    commands,
    configuration,
    contributions,
  })
}

/// Identifiers are dot-separated segments of lowercase letters, digits, and
/// hyphens, like `publisher.name`.
fn validate_id(id: &str) -> Result<(), ManifestError> {
  let segments: Vec<&str> = id.split('.').collect();
  let valid = (3..=64).contains(&id.len())
    && segments.len() >= 2
    && segments.iter().all(|segment| {
      !segment.is_empty()
        && !segment.starts_with('-')
        && segment
          .chars()
          .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    });
  if valid {
    Ok(())
  } else {
    Err(invalid(format!(
      "id {id:?} must look like \"publisher.name\": lowercase letters, digits, and '-', separated by '.'"
    )))
  }
}

fn resolve_entrypoint(
  dir: &Path,
  entrypoint: &str,
  development: bool,
) -> Result<PathBuf, ManifestError> {
  let relative = Path::new(entrypoint);
  if entrypoint.is_empty() || relative.is_absolute() || relative.has_root() {
    return Err(invalid(
      "entrypoint must be a path relative to the manifest",
    ));
  }
  let escapes = relative
    .components()
    .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir));
  if escapes && !development {
    return Err(invalid(
      "entrypoint must be inside the extension's folder (only development extensions may use '..')",
    ));
  }
  Ok(dir.join(relative))
}

fn parse_property(key: &str, raw: RawProperty) -> Result<ConfigProperty, ManifestError> {
  let kind = match raw.kind.as_str() {
    "boolean" => ConfigType::Boolean,
    "integer" => ConfigType::Integer,
    "number" => ConfigType::Number,
    "string" => ConfigType::String,
    other => {
      return Err(invalid(format!(
        "configuration.{key}: unknown type {other:?} (expected boolean, integer, number, or string)"
      )));
    }
  };
  if raw.choices.is_some() && kind != ConfigType::String {
    return Err(invalid(format!(
      "configuration.{key}: choices are only allowed for strings"
    )));
  }
  if (raw.minimum.is_some() || raw.maximum.is_some())
    && !matches!(kind, ConfigType::Integer | ConfigType::Number)
  {
    return Err(invalid(format!(
      "configuration.{key}: minimum and maximum are only allowed for numbers"
    )));
  }
  let mut property = ConfigProperty {
    kind,
    default: ConfigValue::Boolean(false),
    description: raw.description,
    minimum: raw.minimum,
    maximum: raw.maximum,
    choices: raw.choices,
  };
  property.default = property
    .check(&raw.default)
    .map_err(|error| invalid(format!("configuration.{key}: default {error}")))?;
  Ok(property)
}

#[cfg(test)]
mod tests {
  use super::*;

  const VALID: &str = r#"
id = "example.tidy"
name = "Tidy"
version = "0.1.0"
api_version = "1"
entrypoint = "tidy.wasm"
activation = ["on-startup", "on-language:rust", "on-command:example.tidy.trim"]

[engines]
ferrite = ">=0.1.0"

[permissions]
documents_read = true

[[commands]]
id = "example.tidy.trim"
title = "Trim"

[configuration.width]
type = "integer"
default = 100
minimum = 0
"#;

  fn editor() -> Version {
    Version::new(0, 1, 0)
  }

  fn parse_text(text: &str) -> Result<Manifest, ManifestError> {
    parse(text, Path::new("/ext"), &editor(), false)
  }

  fn error_of(text: &str) -> String {
    parse_text(text).expect_err("rejected").to_string()
  }

  #[test]
  fn parses_a_complete_manifest() {
    let manifest = parse_text(VALID).expect("valid");
    assert_eq!(manifest.id, "example.tidy");
    assert_eq!(manifest.version, Version::new(0, 1, 0));
    assert_eq!(
      manifest.entrypoint,
      Some(Path::new("/ext").join("tidy.wasm"))
    );
    assert!(manifest.activates_on_startup());
    assert!(manifest.activates_for_language("rust"));
    assert_eq!(manifest.permissions, vec![Permission::DocumentsRead]);
    assert_eq!(manifest.commands[0].category, "Tidy");
    assert_eq!(
      manifest.configuration["width"].default,
      ConfigValue::Integer(100)
    );
  }

  #[test]
  fn rejects_unsupported_api_and_editor_versions() {
    let api = VALID.replace("api_version = \"1\"", "api_version = \"2\"");
    assert!(matches!(
      parse_text(&api),
      Err(ManifestError::UnsupportedApi { .. })
    ));
    assert!(error_of(&api).contains("supports 1"));
    let engine = VALID.replace(">=0.1.0", ">=9.0");
    assert!(matches!(
      parse_text(&engine),
      Err(ManifestError::IncompatibleEditor { .. })
    ));
  }

  #[test]
  fn rejects_unknown_fields_and_permissions() {
    let field = format!("{VALID}\nunknown = 1\n");
    assert!(error_of(&field).contains("unknown"));
    let permission = VALID.replace("documents_read = true", "network = true");
    assert!(error_of(&permission).contains("network"));
  }

  #[test]
  fn validates_identifiers_and_commands() {
    for id in ["Tidy", "tidy", "example..tidy", "example.Tidy", "a.b c"] {
      let text = VALID.replace("id = \"example.tidy\"", &format!("id = {id:?}"));
      assert!(error_of(&text).contains("publisher.name"), "{id}");
    }
    let foreign = VALID.replace("id = \"example.tidy.trim\"", "id = \"other.trim\"");
    assert!(error_of(&foreign).contains("must start with"));
    let undeclared = VALID.replace("on-command:example.tidy.trim", "on-command:example.tidy.x");
    assert!(error_of(&undeclared).contains("not declared"));
    let event = VALID.replace("on-startup", "on-boot");
    assert!(error_of(&event).contains("unknown activation"));
  }

  #[test]
  fn confines_entrypoints_unless_in_development() {
    let outside = VALID.replace("tidy.wasm", "../tidy.wasm");
    assert!(error_of(&outside).contains("inside the extension's folder"));
    assert!(parse(&outside, Path::new("/ext"), &editor(), true).is_ok());
    let absolute = VALID.replace("\"tidy.wasm\"", "\"/tmp/tidy.wasm\"");
    assert!(error_of(&absolute).contains("relative"));
  }

  #[test]
  fn checks_configuration_defaults() {
    let wrong_type = VALID.replace("default = 100", "default = \"wide\"");
    assert!(error_of(&wrong_type).contains("configuration.width"));
    let out_of_range = VALID.replace("default = 100", "default = -1");
    assert!(error_of(&out_of_range).contains("at least 0"));
    let unknown = VALID.replace("type = \"integer\"", "type = \"color\"");
    assert!(error_of(&unknown).contains("unknown type"));
  }

  const WORKBENCH: &str = r##"
[[keybindings]]
command = "example.tidy.trim"
key = "Shift+Ctrl+T"
mac = "mod+shift+t"
when = "editorFocus"

[[menus]]
location = "editor/context"
command = "example.tidy.trim"

[[status_items]]
id = "count"
text = "3 problems"
command = "example.tidy.trim"

[[views]]
id = "problems"
title = "Problems"
icon = "check"

[decorations.todo]
background = "#E5C07B33"
underline = "dotted"
[decorations.todo.light]
background = "#98680122"

[[previews]]
id = "report"
icon = "bug"

[[previews]]
id = "summary"
"##;

  #[test]
  fn parses_workbench_contributions() {
    let manifest = parse_text(&format!("{VALID}{WORKBENCH}")).expect("valid");
    let contributions = &manifest.contributions;
    let binding = &contributions.keybindings[0];
    assert_eq!(binding.key, "ctrl+shift+t");
    assert_eq!(binding.mac.as_deref(), Some("mod+shift+t"));
    assert!(binding.editor_focus);
    assert_eq!(
      contributions.menus[0].location,
      crate::contributions::MenuLocation::EditorContext
    );
    let item = &contributions.status_items[0];
    assert_eq!(item.alignment, crate::contributions::Alignment::Left);
    assert_eq!(item.text.as_deref(), Some("3 problems"));
    assert_eq!(contributions.views[0].full_id, "example.tidy.problems");
    let todo = &contributions.decorations[0];
    assert_eq!(todo.key, "example.tidy/todo");
    assert_eq!(todo.dark.background.as_deref(), Some("#e5c07b33"));
    assert_eq!(todo.light.background.as_deref(), Some("#98680122"));
    assert_eq!(contributions.previews[0].full_id, "example.tidy.report");
    assert_eq!(contributions.previews[0].icon, "bug");
    assert_eq!(contributions.previews[1].icon, "book", "the default");
  }

  #[test]
  fn rejects_invalid_workbench_contributions() {
    let with =
      |old: &str, new: &str| error_of(&format!("{VALID}{}", WORKBENCH.replacen(old, new, 1)));
    assert!(
      with(
        "command = \"example.tidy.trim\"\nkey",
        "command = \"other.x\"\nkey"
      )
      .contains("not one of")
    );
    assert!(with("Shift+Ctrl+T", "hyper+t").contains("unknown modifier"));
    assert!(with("editor/context", "title/bar").contains("location"));
    assert!(with("icon = \"check\"", "icon = \"skull\"").contains("icon"));
    assert!(with("#E5C07B33", "red").contains("hex color"));
    assert!(with("underline = \"dotted\"", "underline = \"zigzag\"").contains("underline"));
    assert!(with("icon = \"bug\"", "icon = \"globe\"").contains("previews: icon"));
    assert!(with("id = \"summary\"", "id = \"report\"").contains("declared twice"));
    assert!(with("id = \"summary\"", "id = \"Sum mary\"").contains("previews: id"));
    let view = VALID.replace("\"on-startup\"", "\"on-view:missing\"");
    assert!(error_of(&view).contains("view that is not declared"));
  }

  #[test]
  fn icon_theme_extensions_may_omit_the_entrypoint() {
    let icons = r#"
id = "example.icons"
name = "Icons"
version = "1.0.0"
api_version = "1"

[[icon_themes]]
id = "colorful"
label = "Colorful"
path = "icons/theme.toml"
"#;
    let manifest = parse_text(icons).expect("valid");
    assert!(manifest.entrypoint.is_none());
    let theme = &manifest.contributions.icon_themes[0];
    assert_eq!(theme.id, "example.icons.colorful");
    assert_eq!(theme.label, "Colorful");
    let escaping = icons.replace("icons/theme.toml", "../theme.toml");
    assert!(error_of(&escaping).contains("inside the extension's folder"));
    let twice = format!(
      "{icons}
[[icon_themes]]
id = \"colorful\"
label = \"Again\"
path = \"a.toml\"
"
    );
    assert!(error_of(&twice).contains("declared twice"));
  }

  #[test]
  fn only_theme_extensions_may_omit_the_entrypoint() {
    let themes = r#"
id = "example.themes"
name = "Themes"
version = "1.0.0"
api_version = "1"

[[themes]]
id = "night"
label = "Night"
kind = "dark"
path = "themes/night.toml"
"#;
    let manifest = parse_text(themes).expect("valid");
    assert!(manifest.entrypoint.is_none());
    assert_eq!(manifest.contributions.themes[0].id, "example.themes.night");
    let escaping = themes.replace("themes/night.toml", "../night.toml");
    assert!(error_of(&escaping).contains("inside the extension's folder"));
    let code = VALID.replace("entrypoint = \"tidy.wasm\"\n", "");
    assert!(error_of(&code).contains("entrypoint is required"));
  }

  const LANGUAGES: &str = r#"
id = "example.web"
name = "Web"
version = "1.0.0"
api_version = "1"

[permissions]
language_servers = true

[[languages]]
id = "html"
name = "HTML"
file_extensions = ["html", "HTM"]
grammar = "grammars/tree-sitter-html.wasm"
highlights = "queries/html/highlights.scm"
injections = "queries/html/injections.scm"
folds = "queries/html/folds.scm"
block_comment = ["<!--", "-->"]

[[languages]]
id = "objective-c"
name = "Objective-C"
file_names = ["Podfile"]
grammar = "grammars/objc.wasm"
highlights = "queries/objc.scm"
line_comment = "//"

[[language_servers]]
name = "vscode-html-language-server"
command = "vscode-html-language-server"
args = ["--stdio"]
languages = ["html"]
root_markers = ["package.json"]
"#;

  #[test]
  fn language_extensions_need_no_entrypoint() {
    let manifest = parse_text(LANGUAGES).expect("valid");
    assert!(manifest.entrypoint.is_none());
    assert_eq!(manifest.permissions, vec![Permission::LanguageServers]);
    let html = &manifest.contributions.languages[0];
    assert_eq!(html.id, "html");
    assert_eq!(html.file_extensions, ["html", "htm"]);
    assert_eq!(html.grammar_name, "html");
    assert_eq!(
      html.injections.as_deref(),
      Some(Path::new("queries/html/injections.scm"))
    );
    assert_eq!(
      html.folds.as_deref(),
      Some(Path::new("queries/html/folds.scm"))
    );
    assert_eq!(html.line_comment, None);
    assert_eq!(
      html.block_comment,
      Some(("<!--".to_owned(), "-->".to_owned()))
    );
    let objc = &manifest.contributions.languages[1];
    assert_eq!(objc.grammar_name, "objective_c", "derived from the id");
    assert_eq!(objc.line_comment.as_deref(), Some("//"));
    assert_eq!(objc.file_names, ["Podfile"]);
    let server = &manifest.contributions.language_servers[0];
    assert_eq!(server.command_line(), "vscode-html-language-server --stdio");
    assert_eq!(server.root_markers, ["package.json"]);
  }

  #[test]
  fn rejects_invalid_languages() {
    let with = |old: &str, new: &str| error_of(&LANGUAGES.replacen(old, new, 1));
    assert!(with("id = \"html\"", "id = \"HTML\"").contains("lowercase"));
    assert!(with("\"HTM\"", "\".htm\"").contains("without the leading dot"));
    assert!(with("\"HTM\"", "\"a/b\"").contains("file extension"));
    assert!(with("\"Podfile\"", "\"dir/Podfile\"").contains("without a folder"));
    assert!(with("grammars/tree-sitter-html.wasm", "../html.wasm").contains("inside"));
    assert!(with("grammars/tree-sitter-html.wasm", "html.so").contains(".wasm"));
    assert!(with("queries/html/injections.scm", "/etc/x.scm").contains("inside"));
    assert!(with("queries/html/folds.scm", "../folds.scm").contains("inside"));
    assert!(with("id = \"objective-c\"", "id = \"html\"").contains("declared twice"));
    assert!(with("line_comment = \"//\"", "line_comment = \"# \"").contains("line_comment"));
    assert!(with("\"<!--\", \"-->\"", "\"<!--\", \"\"").contains("block_comment"));
    let name = LANGUAGES.replacen(
      "highlights = \"queries/objc.scm\"",
      "highlights = \"queries/objc.scm\"\ngrammar_name = \"Objective C\"",
      1,
    );
    assert!(error_of(&name).contains("grammar_name"));
  }

  #[test]
  fn language_servers_need_their_permission_and_valid_commands() {
    let with = |old: &str, new: &str| error_of(&LANGUAGES.replacen(old, new, 1));
    assert!(
      with("language_servers = true", "language_servers = false")
        .contains("permissions.language_servers")
    );
    let only_permission = LANGUAGES
      .split("[[language_servers]]")
      .next()
      .expect("head");
    assert!(error_of(only_permission).contains("permissions.language_servers"));
    assert!(
      with(
        "command = \"vscode-html-language-server\"",
        "command = \"/usr/bin/server\""
      )
      .contains("inside")
    );
    assert!(
      with(
        "command = \"vscode-html-language-server\"",
        "command = \"../server\""
      )
      .contains("inside")
    );
    let shipped = LANGUAGES.replacen(
      "command = \"vscode-html-language-server\"",
      "command = \"bin/server\"",
      1,
    );
    assert!(parse_text(&shipped).is_ok(), "a server inside the folder");
    assert!(with("languages = [\"html\"]", "languages = []").contains("must not be empty"));
    assert!(with("[\"package.json\"]", "[\"a/package.json\"]").contains("without a folder"));
    assert!(
      with(
        "name = \"vscode-html-language-server\"",
        "name = \"two words\""
      )
      .contains("name")
    );
    // Code permissions still need code.
    let read = LANGUAGES.replacen(
      "language_servers = true",
      "language_servers = true\ndocuments_read = true",
      1,
    );
    assert!(error_of(&read).contains("entrypoint is required"));
  }

  #[test]
  fn language_files_must_exist() {
    let dir = tempfile::tempdir().expect("temp dir");
    let manifest = LANGUAGES
      .split("[[languages]]\nid = \"objective-c\"")
      .next()
      .expect("html only")
      .to_owned()
      + &LANGUAGES[LANGUAGES.find("[[language_servers]]").expect("servers")..];
    std::fs::write(dir.path().join(MANIFEST_FILE), &manifest).expect("writes");
    let error = load(dir.path(), &editor(), false).expect_err("files are missing");
    assert!(
      error.to_string().contains("tree-sitter-html.wasm"),
      "{error}"
    );
    for file in [
      "grammars/tree-sitter-html.wasm",
      "queries/html/highlights.scm",
      "queries/html/injections.scm",
      "queries/html/folds.scm",
    ] {
      let path = dir.path().join(file);
      std::fs::create_dir_all(path.parent().expect("parent")).expect("creates");
      std::fs::write(path, "").expect("writes");
    }
    assert!(load(dir.path(), &editor(), false).is_ok());
  }
}
