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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandContribution {
  pub id: String,
  pub title: String,
  /// Shown before the title in the command palette; the extension's name
  /// by default.
  pub category: String,
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
  /// The component or module to load, resolved against `dir`.
  pub entrypoint: PathBuf,
  pub activation: Vec<Activation>,
  pub permissions: Vec<Permission>,
  pub commands: Vec<CommandContribution>,
  pub configuration: BTreeMap<String, ConfigProperty>,
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
  entrypoint: String,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommand {
  id: String,
  title: String,
  category: Option<String>,
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
  let manifest = parse(&text, dir, editor_version, development)?;
  let entry = std::fs::metadata(&manifest.entrypoint).map_err(|source| ManifestError::Read {
    path: manifest.entrypoint.clone(),
    source,
  })?;
  if !entry.is_file() {
    return Err(invalid(format!(
      "entrypoint {} is not a file",
      manifest.entrypoint.display()
    )));
  }
  if entry.len() > MAX_ENTRYPOINT_BYTES {
    return Err(invalid(format!(
      "entrypoint is larger than {} MiB",
      MAX_ENTRYPOINT_BYTES / 1024 / 1024
    )));
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

  let entrypoint = resolve_entrypoint(dir, &raw.entrypoint, development)?;

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
    } else {
      return Err(invalid(format!(
        "unknown activation event {event:?} (expected \"on-startup\", \"on-command:<id>\", or \"on-language:<id>\")"
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
    assert_eq!(manifest.entrypoint, Path::new("/ext").join("tidy.wasm"));
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
}
