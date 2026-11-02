//! Workbench contributions declared in a manifest: themes, file icon themes,
//! keybindings, menu items, status bar items, side bar views, decoration
//! types, previews, languages, and language servers.
//!
//! Contributions are data: the workbench renders them, and extensions only
//! fill in content (status text, view items, decorated ranges, preview
//! nodes) through the `workbench` interface. Every value is validated here, so nothing an
//! extension declares reaches the frontend as markup or arbitrary CSS:
//! colors must be hex colors and theme keys must be known tokens, and icons
//! are images (see [`crate::icons`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock};

use serde::{Deserialize, Serialize};

use crate::icons::{self, IconTheme};
use crate::keys;
use crate::manifest::{CommandContribution, ManifestError};

/// The semantic color tokens a theme may set (`--color-<token>`), shared
/// with the frontend, which checks them against `styles/tokens.css`.
pub static THEME_TOKENS: LazyLock<BTreeSet<String>> = LazyLock::new(|| {
  serde_json::from_str(include_str!("../../../src/themes/tokens.json")).unwrap_or_default()
});
/// Icons a view can use in the activity bar, and a preview on its tab.
pub const VIEW_ICONS: &[&str] = &[
  "list", "check", "bolt", "book", "bug", "tag", "star", "flag", "bell",
];
const MAX_THEME_FILE_BYTES: u64 = 256 * 1024;

fn invalid(message: impl Into<String>) -> ManifestError {
  ManifestError::Invalid(message.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
  Dark,
  Light,
}

/// A color theme: a base (dark or light) and colors that override its tokens.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeContribution {
  /// `<extension id>.<theme id>`, as written in `workbench.theme`.
  pub id: String,
  pub label: String,
  pub kind: ThemeKind,
  /// Token (without `--color-`) to hex color.
  pub colors: BTreeMap<String, String>,
  #[serde(skip)]
  pub path: PathBuf,
}

/// A file icon theme. Its icons are loaded with the manifest but only sent
/// to the frontend on request (see `ExtensionHost::icon_theme`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconThemeContribution {
  /// `<extension id>.<icon theme id>`, as written in `workbench.icon_theme`.
  pub id: String,
  pub label: String,
  #[serde(skip)]
  pub path: PathBuf,
  #[serde(skip)]
  pub theme: Arc<IconTheme>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeybindingContribution {
  pub command: String,
  pub key: String,
  /// A different binding on macOS.
  pub mac: Option<String>,
  /// Only while the editor has focus.
  pub editor_focus: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MenuLocation {
  /// The editor's context menu.
  #[serde(rename = "editor/context")]
  EditorContext,
  /// The explorer's context menu for a file or folder; the command gets its
  /// path as the argument.
  #[serde(rename = "explorer/context")]
  ExplorerContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuContribution {
  pub location: MenuLocation,
  pub command: String,
  /// Items are grouped by this, then sorted by title.
  pub group: String,
  /// In the explorer: shown for files.
  pub for_files: bool,
  /// In the explorer: shown for folders.
  pub for_folders: bool,
  /// File name patterns (`*.md`, `*.{ts,tsx}`, `Cargo.toml`), compared
  /// ignoring case: the item is shown only for files whose names match one
  /// (the active document's, in the editor). Empty: every file.
  pub patterns: Vec<String>,
}

/// Most patterns in a menu item, and the longest pattern.
const MAX_MENU_PATTERNS: usize = 20;
const MAX_PATTERN_CHARS: usize = 100;

/// Checks a file name pattern: `*` (any characters), `?` (one character),
/// and `{a,b}` (either), matching a file's name, not its path.
fn check_pattern(pattern: &str) -> Result<(), String> {
  if pattern.is_empty() || pattern.chars().count() > MAX_PATTERN_CHARS {
    return Err(format!("must be 1 to {MAX_PATTERN_CHARS} characters"));
  }
  if pattern.contains(['/', '\\']) {
    return Err("matches a file's name, so it can't contain / or \\".into());
  }
  if pattern.contains(['[', ']']) || pattern.chars().any(char::is_control) {
    return Err("can only use *, ?, and {a,b}".into());
  }
  let mut open = false;
  for c in pattern.chars() {
    match c {
      '{' if open => return Err("can't nest { }".into()),
      '{' => open = true,
      '}' if !open => return Err("has a } without a {".into()),
      '}' => open = false,
      _ => {}
    }
  }
  if open {
    return Err("has a { without a }".into());
  }
  Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
  Left,
  Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusItemContribution {
  pub id: String,
  pub alignment: Alignment,
  /// Higher priorities are placed further left within their side.
  pub priority: i32,
  /// Shown until the extension sets other content.
  pub text: Option<String>,
  pub tooltip: Option<String>,
  pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewContribution {
  /// The id the extension uses.
  pub id: String,
  /// `<extension id>.<view id>`, unique in the workbench.
  pub full_id: String,
  pub title: String,
  pub icon: String,
}

/// A kind of preview: a tab of formatted text the extension fills in, such
/// as a rendered Markdown file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewContribution {
  /// The id the extension uses.
  pub id: String,
  /// `<extension id>.<preview id>`.
  pub full_id: String,
  /// Shown on its tabs.
  pub icon: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnderlineStyle {
  Solid,
  Wavy,
  Dotted,
  Dashed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecorationColors {
  pub background: Option<String>,
  pub color: Option<String>,
  pub border: Option<String>,
  pub underline_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecorationType {
  /// The name the extension uses.
  pub name: String,
  /// `<extension id>/<name>`, as decorations are tagged in views.
  pub key: String,
  pub dark: DecorationColors,
  pub light: DecorationColors,
  pub underline: Option<UnderlineStyle>,
  pub bold: bool,
  pub italic: bool,
}

/// A language: which files it is for, and a Tree-sitter grammar compiled to
/// WebAssembly with its queries. Paths are relative to the extension's
/// folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageContribution {
  /// The language id, e.g. `html`. Not prefixed with the extension's id:
  /// language servers, settings, and other languages' injections use it.
  pub id: String,
  pub name: String,
  /// Lowercase, without the leading dot.
  pub file_extensions: Vec<String>,
  pub file_names: Vec<String>,
  #[serde(skip)]
  pub grammar: PathBuf,
  /// The WebAssembly module exports `tree_sitter_<grammar_name>`.
  #[serde(skip)]
  pub grammar_name: String,
  #[serde(skip)]
  pub highlights: PathBuf,
  #[serde(skip)]
  pub injections: Option<PathBuf>,
  /// The fold query (`folds.scm`), which captures foldable nodes as `@fold`.
  #[serde(skip)]
  pub folds: Option<PathBuf>,
  /// Starts a comment to the end of the line, e.g. `//`.
  pub line_comment: Option<String>,
  /// Open and close a block comment, e.g. `/*` and `*/`.
  pub block_comment: Option<(String, String)>,
}

/// A language server the editor starts for documents of some languages,
/// once the user grants `language-servers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageServerContribution {
  pub name: String,
  /// A program on the `PATH`, or a path inside the extension's folder
  /// (made absolute by `ExtensionHost::language_servers`).
  pub command: String,
  pub args: Vec<String>,
  /// Language ids, e.g. `html`.
  pub languages: Vec<String>,
  pub root_markers: Vec<String>,
}

impl LanguageServerContribution {
  /// The command line, as shown when asking for permission.
  pub fn command_line(&self) -> String {
    std::iter::once(&self.command)
      .chain(&self.args)
      .map(|part| {
        if part.is_empty() || part.contains(char::is_whitespace) {
          format!("{part:?}")
        } else {
          part.clone()
        }
      })
      .collect::<Vec<_>>()
      .join(" ")
  }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contributions {
  pub themes: Vec<ThemeContribution>,
  pub icon_themes: Vec<IconThemeContribution>,
  pub keybindings: Vec<KeybindingContribution>,
  pub menus: Vec<MenuContribution>,
  pub status_items: Vec<StatusItemContribution>,
  pub views: Vec<ViewContribution>,
  pub decorations: Vec<DecorationType>,
  pub previews: Vec<PreviewContribution>,
  pub languages: Vec<LanguageContribution>,
  pub language_servers: Vec<LanguageServerContribution>,
}

impl Contributions {
  /// Whether anything needs the extension's code to run.
  pub fn needs_code(&self) -> bool {
    !(self.keybindings.is_empty()
      && self.menus.is_empty()
      && self.status_items.is_empty()
      && self.views.is_empty()
      && self.decorations.is_empty()
      && self.previews.is_empty())
  }

  pub fn preview(&self, id: &str) -> Option<&PreviewContribution> {
    self.previews.iter().find(|preview| preview.id == id)
  }

  pub fn view(&self, id: &str) -> Option<&ViewContribution> {
    self.views.iter().find(|view| view.id == id)
  }

  pub fn status_item(&self, id: &str) -> Option<&StatusItemContribution> {
    self.status_items.iter().find(|item| item.id == id)
  }

  pub fn decoration(&self, name: &str) -> Option<&DecorationType> {
    self
      .decorations
      .iter()
      .find(|decoration| decoration.name == name)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawTheme {
  id: String,
  label: String,
  kind: String,
  path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawIconTheme {
  id: String,
  label: String,
  path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawKeybinding {
  command: String,
  key: String,
  mac: Option<String>,
  when: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawMenu {
  location: String,
  command: String,
  group: Option<String>,
  targets: Option<Vec<String>>,
  patterns: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawStatusItem {
  id: String,
  alignment: Option<String>,
  priority: Option<i32>,
  text: Option<String>,
  tooltip: Option<String>,
  command: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawView {
  pub(crate) id: String,
  title: String,
  icon: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPreview {
  id: String,
  icon: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RawDecorationColors {
  background: Option<String>,
  color: Option<String>,
  border: Option<String>,
  underline_color: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawDecoration {
  background: Option<String>,
  color: Option<String>,
  border: Option<String>,
  underline_color: Option<String>,
  underline: Option<String>,
  #[serde(default)]
  bold: bool,
  #[serde(default)]
  italic: bool,
  light: Option<RawDecorationColors>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLanguage {
  id: String,
  name: String,
  #[serde(default)]
  file_extensions: Vec<String>,
  #[serde(default)]
  file_names: Vec<String>,
  grammar: String,
  grammar_name: Option<String>,
  highlights: String,
  injections: Option<String>,
  folds: Option<String>,
  line_comment: Option<String>,
  block_comment: Option<(String, String)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLanguageServer {
  name: String,
  command: String,
  #[serde(default)]
  args: Vec<String>,
  languages: Vec<String>,
  #[serde(default)]
  root_markers: Vec<String>,
}

/// The contribution sections of a manifest, as written.
#[derive(Default)]
pub struct RawContributions {
  pub themes: Vec<RawTheme>,
  pub icon_themes: Vec<RawIconTheme>,
  pub keybindings: Vec<RawKeybinding>,
  pub menus: Vec<RawMenu>,
  pub status_items: Vec<RawStatusItem>,
  pub views: Vec<RawView>,
  pub decorations: BTreeMap<String, RawDecoration>,
  pub previews: Vec<RawPreview>,
  pub languages: Vec<RawLanguage>,
  pub language_servers: Vec<RawLanguageServer>,
}

/// Validates contributions. Theme colors and icons are read later, by
/// [`load_theme_colors`] and [`load_icon_theme`].
pub fn parse(
  raw: RawContributions,
  extension: &str,
  commands: &[CommandContribution],
) -> Result<Contributions, ManifestError> {
  let declared = |section: &str, command: &str| {
    if commands.iter().any(|c| c.id == command) {
      Ok(command.to_owned())
    } else {
      Err(invalid(format!(
        "{section}: command {command:?} is not one of this extension's commands"
      )))
    }
  };
  let limit = |section: &str, count: usize, max: usize| {
    if count > max {
      Err(invalid(format!("at most {max} {section} can be declared")))
    } else {
      Ok(())
    }
  };
  limit("themes", raw.themes.len(), 20)?;
  limit("icon_themes", raw.icon_themes.len(), 5)?;
  limit("keybindings", raw.keybindings.len(), 100)?;
  limit("menus", raw.menus.len(), 50)?;
  limit("status_items", raw.status_items.len(), 20)?;
  limit("views", raw.views.len(), 10)?;
  limit("decorations", raw.decorations.len(), 20)?;
  limit("previews", raw.previews.len(), 10)?;
  limit("languages", raw.languages.len(), 20)?;
  limit("language_servers", raw.language_servers.len(), 10)?;

  let mut contributions = Contributions::default();
  for theme in raw.themes {
    local_id("themes", &theme.id)?;
    let kind = match theme.kind.as_str() {
      "dark" => ThemeKind::Dark,
      "light" => ThemeKind::Light,
      other => {
        return Err(invalid(format!(
          "themes: kind {other:?} must be \"dark\" or \"light\""
        )));
      }
    };
    let label = text("themes", "label", &theme.label, 64)?;
    let path = inside("themes", &theme.path)?;
    let id = format!("{extension}.{}", theme.id);
    unique("themes", contributions.themes.iter().map(|t| &t.id), &id)?;
    contributions.themes.push(ThemeContribution {
      id,
      label,
      kind,
      colors: BTreeMap::new(),
      path,
    });
  }

  for theme in raw.icon_themes {
    local_id("icon_themes", &theme.id)?;
    let label = text("icon_themes", "label", &theme.label, 64)?;
    let path = inside("icon_themes", &theme.path)?;
    let id = format!("{extension}.{}", theme.id);
    unique(
      "icon_themes",
      contributions.icon_themes.iter().map(|t| &t.id),
      &id,
    )?;
    contributions.icon_themes.push(IconThemeContribution {
      id,
      label,
      path,
      theme: Arc::default(),
    });
  }

  for binding in raw.keybindings {
    let command = declared("keybindings", &binding.command)?;
    let key =
      keys::normalize(&binding.key).map_err(|error| invalid(format!("keybindings: {error}")))?;
    let mac = binding
      .mac
      .as_deref()
      .map(keys::normalize)
      .transpose()
      .map_err(|error| invalid(format!("keybindings: {error}")))?;
    let editor_focus = match binding.when.as_deref() {
      None => false,
      Some("editorFocus") => true,
      Some(other) => {
        return Err(invalid(format!(
          "keybindings: when {other:?} is not supported (only \"editorFocus\")"
        )));
      }
    };
    contributions.keybindings.push(KeybindingContribution {
      command,
      key,
      mac,
      editor_focus,
    });
  }

  for menu in raw.menus {
    let location = match menu.location.as_str() {
      "editor/context" => MenuLocation::EditorContext,
      "explorer/context" => MenuLocation::ExplorerContext,
      other => {
        return Err(invalid(format!(
          "menus: location {other:?} must be \"editor/context\" or \"explorer/context\""
        )));
      }
    };
    let command = declared("menus", &menu.command)?;
    let (for_files, for_folders) = match (location, menu.targets) {
      (_, None) => (true, false),
      (MenuLocation::EditorContext, Some(_)) => {
        return Err(invalid(format!(
          "menus: {command}: targets only apply to \"explorer/context\""
        )));
      }
      (MenuLocation::ExplorerContext, Some(targets)) => {
        if let Some(other) = targets.iter().find(|t| *t != "files" && *t != "folders") {
          return Err(invalid(format!(
            "menus: {command}: target {other:?} must be \"files\" or \"folders\""
          )));
        }
        if targets.is_empty() {
          return Err(invalid(format!(
            "menus: {command}: targets must list \"files\", \"folders\", or both"
          )));
        }
        (
          targets.iter().any(|t| t == "files"),
          targets.iter().any(|t| t == "folders"),
        )
      }
    };
    let patterns = menu.patterns.unwrap_or_default();
    if patterns.len() > MAX_MENU_PATTERNS {
      return Err(invalid(format!(
        "menus: {command}: at most {MAX_MENU_PATTERNS} patterns"
      )));
    }
    if !patterns.is_empty() && !for_files {
      return Err(invalid(format!(
        "menus: {command}: patterns only apply to files"
      )));
    }
    for pattern in &patterns {
      check_pattern(pattern)
        .map_err(|problem| invalid(format!("menus: {command}: pattern {pattern:?} {problem}")))?;
    }
    contributions.menus.push(MenuContribution {
      location,
      command,
      group: menu.group.unwrap_or_default(),
      for_files,
      for_folders,
      patterns,
    });
  }

  for item in raw.status_items {
    local_id("status_items", &item.id)?;
    unique(
      "status_items",
      contributions.status_items.iter().map(|i| &i.id),
      &item.id,
    )?;
    let alignment = match item.alignment.as_deref() {
      None | Some("left") => Alignment::Left,
      Some("right") => Alignment::Right,
      Some(other) => {
        return Err(invalid(format!(
          "status_items: alignment {other:?} must be \"left\" or \"right\""
        )));
      }
    };
    contributions.status_items.push(StatusItemContribution {
      id: item.id,
      alignment,
      priority: item.priority.unwrap_or(0),
      text: item
        .text
        .map(|value| text("status_items", "text", &value, 100))
        .transpose()?,
      tooltip: item
        .tooltip
        .map(|value| text("status_items", "tooltip", &value, 500))
        .transpose()?,
      command: item
        .command
        .map(|command| declared("status_items", &command))
        .transpose()?,
    });
  }

  for view in raw.views {
    local_id("views", &view.id)?;
    unique("views", contributions.views.iter().map(|v| &v.id), &view.id)?;
    let icon = view.icon.unwrap_or_else(|| "list".into());
    if !VIEW_ICONS.contains(&icon.as_str()) {
      return Err(invalid(format!(
        "views: icon {icon:?} must be one of {}",
        VIEW_ICONS.join(", ")
      )));
    }
    contributions.views.push(ViewContribution {
      full_id: format!("{extension}.{}", view.id),
      title: text("views", "title", &view.title, 40)?,
      id: view.id,
      icon,
    });
  }

  for (name, decoration) in raw.decorations {
    local_id("decorations", &name)?;
    let section = format!("decorations.{name}");
    let base = RawDecorationColors {
      background: decoration.background,
      color: decoration.color,
      border: decoration.border,
      underline_color: decoration.underline_color,
    };
    let dark = colors(&section, &base)?;
    let light = match &decoration.light {
      Some(light) => {
        let overrides = colors(&format!("{section}.light"), light)?;
        DecorationColors {
          background: overrides.background.or_else(|| dark.background.clone()),
          color: overrides.color.or_else(|| dark.color.clone()),
          border: overrides.border.or_else(|| dark.border.clone()),
          underline_color: overrides
            .underline_color
            .or_else(|| dark.underline_color.clone()),
        }
      }
      None => dark.clone(),
    };
    let underline = match decoration.underline.as_deref() {
      None => None,
      Some("solid") => Some(UnderlineStyle::Solid),
      Some("wavy") => Some(UnderlineStyle::Wavy),
      Some("dotted") => Some(UnderlineStyle::Dotted),
      Some("dashed") => Some(UnderlineStyle::Dashed),
      Some(other) => {
        return Err(invalid(format!(
          "{section}: underline {other:?} must be solid, wavy, dotted, or dashed"
        )));
      }
    };
    contributions.decorations.push(DecorationType {
      key: format!("{extension}/{name}"),
      name,
      dark,
      light,
      underline,
      bold: decoration.bold,
      italic: decoration.italic,
    });
  }

  for preview in raw.previews {
    local_id("previews", &preview.id)?;
    unique(
      "previews",
      contributions.previews.iter().map(|p| &p.id),
      &preview.id,
    )?;
    let icon = preview.icon.unwrap_or_else(|| "book".into());
    if !VIEW_ICONS.contains(&icon.as_str()) {
      return Err(invalid(format!(
        "previews: icon {icon:?} must be one of {}",
        VIEW_ICONS.join(", ")
      )));
    }
    contributions.previews.push(PreviewContribution {
      full_id: format!("{extension}.{}", preview.id),
      id: preview.id,
      icon,
    });
  }

  for language in raw.languages {
    local_id("languages", &language.id)?;
    unique(
      "languages",
      contributions.languages.iter().map(|l| &l.id),
      &language.id,
    )?;
    let section = format!("languages.{}", language.id);
    limit(
      &format!("{section}: file_extensions"),
      language.file_extensions.len(),
      50,
    )?;
    limit(
      &format!("{section}: file_names"),
      language.file_names.len(),
      50,
    )?;
    let file_extensions = language
      .file_extensions
      .iter()
      .map(|extension| file_extension(&section, extension))
      .collect::<Result<_, _>>()?;
    let file_names = language
      .file_names
      .iter()
      .map(|name| file_name(&section, name))
      .collect::<Result<_, _>>()?;
    let grammar = inside(&section, &language.grammar)?;
    if grammar
      .extension()
      .is_none_or(|extension| extension != "wasm")
    {
      return Err(invalid(format!(
        "{section}: grammar {:?} must be a .wasm file built with `tree-sitter build --wasm`",
        language.grammar
      )));
    }
    let grammar_name = language
      .grammar_name
      .unwrap_or_else(|| language.id.replace('-', "_"));
    let valid_name = (1..=64).contains(&grammar_name.len())
      && grammar_name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !valid_name {
      return Err(invalid(format!(
        "{section}: grammar_name {grammar_name:?} must be lowercase letters, digits, and '_'"
      )));
    }
    contributions.languages.push(LanguageContribution {
      name: text(&section, "name", &language.name, 64)?,
      file_extensions,
      file_names,
      grammar,
      grammar_name,
      highlights: inside(&section, &language.highlights)?,
      injections: language
        .injections
        .as_deref()
        .map(|path| inside(&section, path))
        .transpose()?,
      folds: language
        .folds
        .as_deref()
        .map(|path| inside(&section, path))
        .transpose()?,
      line_comment: language
        .line_comment
        .as_deref()
        .map(|token| comment_token(&section, "line_comment", token))
        .transpose()?,
      block_comment: language
        .block_comment
        .as_ref()
        .map(|(open, close)| {
          Ok::<_, ManifestError>((
            comment_token(&section, "block_comment", open)?,
            comment_token(&section, "block_comment", close)?,
          ))
        })
        .transpose()?,
      id: language.id,
    });
  }

  for server in raw.language_servers {
    let valid_name = (1..=64).contains(&server.name.len())
      && server
        .name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !valid_name {
      return Err(invalid(format!(
        "language_servers: name {:?} must be 1 to 64 letters, digits, '-', '_', or '.'",
        server.name
      )));
    }
    unique(
      "language_servers",
      contributions.language_servers.iter().map(|s| &s.name),
      &server.name,
    )?;
    let section = format!("language_servers.{}", server.name);
    // A program to find on the PATH, or one the extension ships.
    if server.command.contains(['/', '\\']) {
      inside(&section, &server.command)?;
    } else {
      file_name(&section, &server.command)?;
    }
    limit(&format!("{section}: args"), server.args.len(), 32)?;
    if server
      .args
      .iter()
      .any(|arg| arg.chars().count() > 1000 || arg.contains('\0'))
    {
      return Err(invalid(format!(
        "{section}: arguments must be at most 1000 characters"
      )));
    }
    if server.languages.is_empty() {
      return Err(invalid(format!("{section}: languages must not be empty")));
    }
    limit(&format!("{section}: languages"), server.languages.len(), 20)?;
    for language in &server.languages {
      local_id(&section, language)?;
    }
    limit(
      &format!("{section}: root_markers"),
      server.root_markers.len(),
      20,
    )?;
    for marker in &server.root_markers {
      file_name(&section, marker)?;
    }
    contributions
      .language_servers
      .push(LanguageServerContribution {
        name: server.name,
        command: server.command,
        args: server.args,
        languages: server.languages,
        root_markers: server.root_markers,
      });
  }
  Ok(contributions)
}

/// Checks that a language's grammar and query files exist. They are read
/// when a file of the language is first opened.
pub fn check_language_files(
  dir: &Path,
  language: &LanguageContribution,
) -> Result<(), ManifestError> {
  let paths = [&language.grammar, &language.highlights]
    .into_iter()
    .chain(&language.injections)
    .chain(&language.folds);
  for path in paths {
    let full = dir.join(path);
    let metadata = std::fs::metadata(&full).map_err(|source| ManifestError::Read {
      path: full.clone(),
      source,
    })?;
    if !metadata.is_file() {
      return Err(invalid(format!(
        "languages.{}: {} is not a file",
        language.id,
        path.display()
      )));
    }
  }
  Ok(())
}

/// A file extension without the leading dot, lowercased.
fn file_extension(section: &str, extension: &str) -> Result<String, ManifestError> {
  let valid = (1..=32).contains(&extension.len())
    && !extension.starts_with('.')
    && !extension.ends_with('.')
    && extension
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
  if valid {
    Ok(extension.to_ascii_lowercase())
  } else {
    Err(invalid(format!(
      "{section}: file extension {extension:?} must be letters, digits, '.', '-', '_', or '+', without the leading dot"
    )))
  }
}

/// A file name without a folder: 1 to 128 characters.
fn file_name(section: &str, name: &str) -> Result<String, ManifestError> {
  let valid = (1..=128).contains(&name.len())
    && !matches!(name, "." | "..")
    && !name.contains(['/', '\\', '\0']);
  if valid {
    Ok(name.to_owned())
  } else {
    Err(invalid(format!(
      "{section}: {name:?} must be a file name without a folder"
    )))
  }
}

/// Reads and checks a theme's colors from its file:
/// `[colors]` mapping known tokens to hex colors.
pub fn load_theme_colors(dir: &Path, theme: &mut ThemeContribution) -> Result<(), ManifestError> {
  #[derive(Deserialize)]
  #[serde(deny_unknown_fields)]
  struct ThemeFile {
    colors: BTreeMap<String, String>,
  }
  let path = dir.join(&theme.path);
  let read = |source| ManifestError::Read {
    path: path.clone(),
    source,
  };
  if std::fs::metadata(&path).map_err(read)?.len() > MAX_THEME_FILE_BYTES {
    return Err(invalid(format!(
      "theme {} is too large",
      theme.path.display()
    )));
  }
  let text = std::fs::read_to_string(&path).map_err(read)?;
  let file: ThemeFile = toml::from_str(&text)
    .map_err(|error| invalid(format!("theme {}: {error}", theme.path.display())))?;
  theme.colors = check_theme_colors(&file.colors)
    .map_err(|error| invalid(format!("theme {}: {error}", theme.path.display())))?;
  Ok(())
}

/// Reads an icon theme's file and icons.
pub fn load_icon_theme(dir: &Path, theme: &mut IconThemeContribution) -> Result<(), ManifestError> {
  let loaded = icons::load(dir, &theme.path)
    .map_err(|error| invalid(format!("icon theme {}: {error}", theme.path.display())))?;
  theme.theme = Arc::new(loaded);
  Ok(())
}

/// Checks theme colors: keys must be known tokens, values hex colors.
/// Also used for the user's `workbench.colors`.
pub fn check_theme_colors(
  colors: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
  let mut checked = BTreeMap::new();
  for (token, value) in colors {
    if !THEME_TOKENS.contains(token) {
      return Err(format!("{token:?} is not a theme color"));
    }
    if !is_hex_color(value) {
      return Err(format!(
        "{token} must be a hex color like \"#1e1e2e\", not {value:?}"
      ));
    }
    checked.insert(token.clone(), value.to_lowercase());
  }
  Ok(checked)
}

/// `#rgb`, `#rgba`, `#rrggbb`, or `#rrggbbaa`.
pub fn is_hex_color(value: &str) -> bool {
  value.strip_prefix('#').is_some_and(|hex| {
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
  })
}

fn colors(section: &str, raw: &RawDecorationColors) -> Result<DecorationColors, ManifestError> {
  let check = |field: &str, value: &Option<String>| match value {
    Some(value) if !is_hex_color(value) => Err(invalid(format!(
      "{section}: {field} must be a hex color like \"#e5c07b\", not {value:?}"
    ))),
    other => Ok(other.as_ref().map(|value| value.to_lowercase())),
  };
  Ok(DecorationColors {
    background: check("background", &raw.background)?,
    color: check("color", &raw.color)?,
    border: check("border", &raw.border)?,
    underline_color: check("underline_color", &raw.underline_color)?,
  })
}

/// Ids local to an extension: lowercase letters, digits, '-', and '_'.
fn local_id(section: &str, id: &str) -> Result<(), ManifestError> {
  let valid = (1..=32).contains(&id.len())
    && id
      .chars()
      .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'));
  if valid {
    Ok(())
  } else {
    Err(invalid(format!(
      "{section}: id {id:?} must be 1 to 32 lowercase letters, digits, '-', or '_'"
    )))
  }
}

fn unique<'a>(
  section: &str,
  mut existing: impl Iterator<Item = &'a String>,
  id: &str,
) -> Result<(), ManifestError> {
  if existing.any(|other| other == id) {
    Err(invalid(format!("{section}: {id:?} is declared twice")))
  } else {
    Ok(())
  }
}

fn text(section: &str, field: &str, value: &str, max: usize) -> Result<String, ManifestError> {
  let value = value.trim();
  if value.is_empty() || value.chars().count() > max {
    Err(invalid(format!(
      "{section}: {field} must be 1 to {max} characters"
    )))
  } else {
    Ok(value.to_owned())
  }
}

/// A comment delimiter: 1 to 8 characters, without whitespace.
fn comment_token(section: &str, field: &str, token: &str) -> Result<String, ManifestError> {
  if (1..=8).contains(&token.chars().count()) && !token.contains(char::is_whitespace) {
    Ok(token.to_owned())
  } else {
    Err(invalid(format!(
      "{section}: {field} delimiters must be 1 to 8 characters without spaces"
    )))
  }
}

/// A relative path that stays inside the extension's folder.
fn inside(section: &str, path: &str) -> Result<PathBuf, ManifestError> {
  let relative = Path::new(path);
  let confined = !path.is_empty()
    && !relative.has_root()
    && relative
      .components()
      .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
  if confined {
    Ok(relative.to_path_buf())
  } else {
    Err(invalid(format!(
      "{section}: path {path:?} must be inside the extension's folder"
    )))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn checks_menu_patterns() {
    for ok in ["*.md", "*.{ts,tsx}", "Cargo.toml", "?.txt", ".*rc"] {
      assert_eq!(check_pattern(ok), Ok(()), "{ok}");
    }
    for bad in [
      "", "src/*.rs", r"**\*.rs", "[ab].rs", "{a,{b}}", "{a", "a}", "a\tb",
    ] {
      assert!(check_pattern(bad).is_err(), "{bad:?}");
    }
  }

  #[test]
  fn the_token_list_is_loaded() {
    assert!(THEME_TOKENS.contains("editor-background"));
    assert!(THEME_TOKENS.len() > 50);
  }

  #[test]
  fn checks_theme_colors() {
    let ok = BTreeMap::from([("editor-background".to_owned(), "#ABCDEF".to_owned())]);
    assert_eq!(
      check_theme_colors(&ok).expect("valid")["editor-background"],
      "#abcdef"
    );
    let token = BTreeMap::from([("editor-backdrop".to_owned(), "#000".to_owned())]);
    assert!(check_theme_colors(&token).is_err());
    for value in ["red", "#12345", "url(x)", "#000;background:url(x)"] {
      let colors = BTreeMap::from([("text".to_owned(), value.to_owned())]);
      assert!(check_theme_colors(&colors).is_err(), "{value}");
    }
  }
}
