//! File icon themes: the explorer's icons for files and folders, chosen by
//! file name, extension, and folder name.
//!
//! Icons are SVG or PNG files in the extension. They are read and checked
//! when the manifest loads and sent to the frontend as `data:` URLs, which
//! it shows as images. An image can't run script or load other resources, so
//! no markup from an extension reaches the page.

use std::collections::BTreeMap;
use std::path::{Component, Path};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

/// The theme file (mappings and icon paths).
const MAX_THEME_FILE_BYTES: u64 = 1024 * 1024;
const MAX_ICON_BYTES: u64 = 64 * 1024;
const MAX_ICONS: usize = 2000;
const MAX_TOTAL_ICON_BYTES: u64 = 8 * 1024 * 1024;
/// File extensions, file names, and folder names, in both sections.
const MAX_ASSOCIATIONS: usize = 10_000;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Which icon (by id) to use for what. Names and extensions are lowercase.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconAssociations {
  /// Files that nothing more specific matches.
  pub file: Option<String>,
  pub folder: Option<String>,
  pub folder_expanded: Option<String>,
  /// Without the leading dot; may have several parts (`d.ts`), and the
  /// longest match wins.
  pub file_extensions: BTreeMap<String, String>,
  pub file_names: BTreeMap<String, String>,
  pub folder_names: BTreeMap<String, String>,
  pub folder_names_expanded: BTreeMap<String, String>,
}

/// A loaded icon theme, as the frontend receives it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconTheme {
  /// Icon id to a `data:` URL.
  pub icons: BTreeMap<String, String>,
  pub associations: IconAssociations,
  /// Used before `associations` while a light color theme is active.
  pub light: IconAssociations,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawAssociations {
  file: Option<String>,
  folder: Option<String>,
  folder_expanded: Option<String>,
  #[serde(default)]
  file_extensions: BTreeMap<String, String>,
  #[serde(default)]
  file_names: BTreeMap<String, String>,
  #[serde(default)]
  folder_names: BTreeMap<String, String>,
  #[serde(default)]
  folder_names_expanded: BTreeMap<String, String>,
}

/// The theme file. The associations are at the top level, and again under
/// `[light]` for light color themes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIconTheme {
  #[serde(default)]
  icons: BTreeMap<String, String>,
  file: Option<String>,
  folder: Option<String>,
  folder_expanded: Option<String>,
  #[serde(default)]
  file_extensions: BTreeMap<String, String>,
  #[serde(default)]
  file_names: BTreeMap<String, String>,
  #[serde(default)]
  folder_names: BTreeMap<String, String>,
  #[serde(default)]
  folder_names_expanded: BTreeMap<String, String>,
  light: Option<RawAssociations>,
}

/// Reads an icon theme from `path` (relative to the extension's folder
/// `dir`) and the icon files it names, relative to the theme file.
pub fn load(dir: &Path, path: &Path) -> Result<IconTheme, String> {
  let file = dir.join(path);
  let size = std::fs::metadata(&file)
    .map_err(|error| format!("could not read it: {error}"))?
    .len();
  if size > MAX_THEME_FILE_BYTES {
    return Err("the file is too large".into());
  }
  let text =
    std::fs::read_to_string(&file).map_err(|error| format!("could not read it: {error}"))?;
  let raw: RawIconTheme = toml::from_str(&text).map_err(|error| error.to_string())?;

  if raw.icons.len() > MAX_ICONS {
    return Err(format!("at most {MAX_ICONS} icons can be declared"));
  }
  let base = file.parent().unwrap_or(dir);
  let mut icons = BTreeMap::new();
  let mut total = 0;
  for (id, icon_path) in &raw.icons {
    icon_id(id)?;
    let (url, bytes) =
      read_icon(base, icon_path).map_err(|error| format!("icons.{id}: {error}"))?;
    total += bytes;
    if total > MAX_TOTAL_ICON_BYTES {
      return Err(format!(
        "the icons are larger than {} MiB in total",
        MAX_TOTAL_ICON_BYTES / 1024 / 1024
      ));
    }
    icons.insert(id.clone(), url);
  }

  let mut count = 0;
  let associations = check_associations(
    "",
    RawAssociations {
      file: raw.file,
      folder: raw.folder,
      folder_expanded: raw.folder_expanded,
      file_extensions: raw.file_extensions,
      file_names: raw.file_names,
      folder_names: raw.folder_names,
      folder_names_expanded: raw.folder_names_expanded,
    },
    &icons,
    &mut count,
  )?;
  let light = match raw.light {
    Some(raw) => check_associations("light.", raw, &icons, &mut count)?,
    None => IconAssociations::default(),
  };
  Ok(IconTheme {
    icons,
    associations,
    light,
  })
}

/// Checks associations: keys are normalized and every value names an icon.
fn check_associations(
  prefix: &str,
  raw: RawAssociations,
  icons: &BTreeMap<String, String>,
  count: &mut usize,
) -> Result<IconAssociations, String> {
  let icon = |field: &str, id: Option<String>| match id {
    Some(id) if !icons.contains_key(&id) => Err(format!(
      "{prefix}{field}: {id:?} is not one of the theme's icons"
    )),
    id => Ok(id),
  };
  let mut table = |field: &str,
                   entries: BTreeMap<String, String>,
                   key: fn(&str) -> Option<String>|
   -> Result<BTreeMap<String, String>, String> {
    *count += entries.len();
    if *count > MAX_ASSOCIATIONS {
      return Err(format!(
        "at most {MAX_ASSOCIATIONS} file extensions, file names, and folder names can be declared"
      ));
    }
    let mut checked = BTreeMap::new();
    for (name, id) in entries {
      let Some(normalized) = key(&name) else {
        return Err(format!("{prefix}{field}: {name:?} is not a valid name"));
      };
      if !icons.contains_key(&id) {
        return Err(format!(
          "{prefix}{field}.{name}: {id:?} is not one of the theme's icons"
        ));
      }
      checked.insert(normalized, id);
    }
    Ok(checked)
  };
  Ok(IconAssociations {
    file_extensions: table("file_extensions", raw.file_extensions, extension_key)?,
    file_names: table("file_names", raw.file_names, name_key)?,
    folder_names: table("folder_names", raw.folder_names, name_key)?,
    folder_names_expanded: table("folder_names_expanded", raw.folder_names_expanded, name_key)?,
    file: icon("file", raw.file)?,
    folder: icon("folder", raw.folder)?,
    folder_expanded: icon("folder_expanded", raw.folder_expanded)?,
  })
}

/// `rs`, `.rs`, or `d.ts`, lowercased without the leading dot.
fn extension_key(key: &str) -> Option<String> {
  let key = key.strip_prefix('.').unwrap_or(key);
  (!key.starts_with('.') && !key.ends_with('.'))
    .then(|| name_key(key))
    .flatten()
}

/// A file or folder name, lowercased.
fn name_key(key: &str) -> Option<String> {
  let valid = (1..=128).contains(&key.chars().count())
    && !key.contains(['/', '\\'])
    && !key.chars().any(char::is_control);
  valid.then(|| key.to_lowercase())
}

fn icon_id(id: &str) -> Result<(), String> {
  let valid = (1..=64).contains(&id.len())
    && id
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
  if valid {
    Ok(())
  } else {
    Err(format!(
      "icon id {id:?} must be 1 to 64 letters, digits, '-', '_', or '.'"
    ))
  }
}

/// Reads an SVG or PNG icon into a `data:` URL, returning it and its size.
fn read_icon(base: &Path, path: &str) -> Result<(String, u64), String> {
  let relative = Path::new(path);
  let confined = !path.is_empty()
    && !relative.has_root()
    && relative
      .components()
      .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
  if !confined {
    return Err(format!(
      "path {path:?} must be inside the icon theme's folder"
    ));
  }
  let file = base.join(relative);
  let size = std::fs::metadata(&file)
    .map_err(|error| format!("could not read {path}: {error}"))?
    .len();
  if size > MAX_ICON_BYTES {
    return Err(format!(
      "{path} is larger than {} KiB",
      MAX_ICON_BYTES / 1024
    ));
  }
  let bytes = std::fs::read(&file).map_err(|error| format!("could not read {path}: {error}"))?;
  let extension = relative
    .extension()
    .map(|extension| extension.to_string_lossy().to_lowercase());
  let mime = match extension.as_deref() {
    Some("svg") if std::str::from_utf8(&bytes).is_ok_and(|text| text.contains("<svg")) => {
      "image/svg+xml"
    }
    Some("png") if bytes.starts_with(PNG_SIGNATURE) => "image/png",
    Some("svg" | "png") => return Err(format!("{path} is not a valid image")),
    _ => return Err(format!("{path} must be an SVG or PNG file")),
  };
  Ok((
    format!("data:{mime};base64,{}", STANDARD.encode(&bytes)),
    bytes.len() as u64,
  ))
}

#[cfg(test)]
mod tests {
  use super::*;

  const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"/>"#;

  fn theme_dir(theme: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("icons/svg")).expect("mkdir");
    std::fs::write(dir.path().join("icons/svg/rust.svg"), SVG).expect("write");
    std::fs::write(dir.path().join("icons/svg/file.svg"), SVG).expect("write");
    std::fs::write(dir.path().join("icons/theme.toml"), theme).expect("write");
    dir
  }

  fn load_theme(theme: &str) -> Result<IconTheme, String> {
    let dir = theme_dir(theme);
    load(dir.path(), Path::new("icons/theme.toml"))
  }

  #[test]
  fn loads_icons_as_data_urls_and_normalizes_associations() {
    let theme = load_theme(
      r#"
file = "file"
[icons]
rust = "svg/rust.svg"
file = "./svg/file.svg"
[file_extensions]
".RS" = "rust"
"d.ts" = "file"
[file_names]
"Cargo.toml" = "rust"
[light]
folder = "file"
"#,
    )
    .expect("loads");
    assert!(theme.icons["rust"].starts_with("data:image/svg+xml;base64,"));
    assert_eq!(theme.associations.file.as_deref(), Some("file"));
    assert_eq!(theme.associations.file_extensions["rs"], "rust");
    assert_eq!(theme.associations.file_extensions["d.ts"], "file");
    assert_eq!(theme.associations.file_names["cargo.toml"], "rust");
    assert_eq!(theme.light.folder.as_deref(), Some("file"));
    assert!(theme.light.file_extensions.is_empty());
  }

  #[test]
  fn associations_must_name_declared_icons() {
    for theme in [
      "file = \"missing\"\n[icons]\nrust = \"svg/rust.svg\"\n",
      "[icons]\nrust = \"svg/rust.svg\"\n[file_extensions]\nrs = \"missing\"\n",
      "[icons]\nrust = \"svg/rust.svg\"\n[light.folder_names]\nsrc = \"missing\"\n",
    ] {
      let error = load_theme(theme).expect_err("unknown icon");
      assert!(error.contains("not one of the theme's icons"), "{error}");
    }
  }

  #[test]
  fn refuses_bad_names_paths_and_files() {
    let cases = [
      (
        "[icons]\nrust = \"../outside.svg\"\n",
        "inside the icon theme's folder",
      ),
      (
        "[icons]\nrust = \"/etc/rust.svg\"\n",
        "inside the icon theme's folder",
      ),
      ("[icons]\nrust = \"svg/missing.svg\"\n", "could not read"),
      ("[icons]\nrust = \"theme.toml\"\n", "SVG or PNG"),
      ("[icons]\n\"bad id\" = \"svg/rust.svg\"\n", "icon id"),
      (
        "[icons]\nrust = \"svg/rust.svg\"\n[file_names]\n\"a/b\" = \"rust\"\n",
        "not a valid name",
      ),
      (
        "[icons]\nrust = \"svg/rust.svg\"\n[file_extensions]\n\"ts.\" = \"rust\"\n",
        "not a valid name",
      ),
      ("unknown = 1\n", "unknown field"),
    ];
    for (theme, expected) in cases {
      let error = load_theme(theme).expect_err(theme);
      assert!(error.contains(expected), "{theme}: {error}");
    }
  }

  #[test]
  fn checks_image_contents_and_sizes() {
    let dir = theme_dir("[icons]\nfake = \"svg/fake.png\"\n");
    std::fs::write(dir.path().join("icons/svg/fake.png"), SVG).expect("write");
    let error = load(dir.path(), Path::new("icons/theme.toml")).expect_err("not a png");
    assert!(error.contains("not a valid image"), "{error}");

    let mut png = PNG_SIGNATURE.to_vec();
    png.extend_from_slice(&[0; 16]);
    std::fs::write(dir.path().join("icons/svg/fake.png"), &png).expect("write");
    let theme = load(dir.path(), Path::new("icons/theme.toml")).expect("a png");
    assert!(theme.icons["fake"].starts_with("data:image/png;base64,iVBORw0KGgo"));

    let large = format!("<svg>{}</svg>", " ".repeat(MAX_ICON_BYTES as usize));
    std::fs::write(dir.path().join("icons/svg/fake.png"), "").expect("write");
    std::fs::write(dir.path().join("icons/svg/big.svg"), large).expect("write");
    std::fs::write(
      dir.path().join("icons/theme.toml"),
      "[icons]\nbig = \"svg/big.svg\"\n",
    )
    .expect("write");
    let error = load(dir.path(), Path::new("icons/theme.toml")).expect_err("too large");
    assert!(error.contains("larger than"), "{error}");
  }
}
