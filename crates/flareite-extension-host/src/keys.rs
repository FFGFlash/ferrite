//! Keybinding syntax, shared by extension manifests and user settings:
//! modifiers and a key joined by `+`, such as `ctrl+shift+p`, `mod+k`, or
//! `alt+f12`. `mod` is Ctrl on Windows and Linux and Cmd on macOS. A chord
//! is two of these separated by a space, such as `mod+k mod+0`: the second
//! is pressed after the first.

const MODIFIERS: &[&str] = &["mod", "ctrl", "alt", "shift", "meta"];
const NAMED_KEYS: &[&str] = &[
  "enter",
  "tab",
  "escape",
  "space",
  "backspace",
  "delete",
  "insert",
  "home",
  "end",
  "pageup",
  "pagedown",
  "arrowup",
  "arrowdown",
  "arrowleft",
  "arrowright",
];

/// Checks a keybinding and returns it in normal form: lowercase, modifiers
/// in a fixed order (`mod`, `ctrl`, `alt`, `shift`, `meta`), and a chord's
/// steps separated by one space.
pub fn normalize(binding: &str) -> Result<String, String> {
  let steps: Vec<&str> = binding.split_whitespace().collect();
  if steps.len() > 2 {
    return Err(format!(
      "keybinding {binding:?} has more than two steps (a chord is two key combinations)"
    ));
  }
  if steps.is_empty() {
    return Err(format!("keybinding {binding:?} is malformed"));
  }
  let steps = steps
    .into_iter()
    .map(|step| normalize_step(binding, step))
    .collect::<Result<Vec<_>, _>>()?;
  Ok(steps.join(" "))
}

/// One key combination of `binding`, in normal form.
fn normalize_step(binding: &str, step: &str) -> Result<String, String> {
  let lower = step.to_lowercase();
  // A literal "+" key is written "mod++".
  let (rest, plus_key) = match lower.strip_suffix("++") {
    Some(rest) => (rest.to_owned(), true),
    None => (lower.clone(), false),
  };
  let mut parts: Vec<&str> = if rest.is_empty() {
    Vec::new()
  } else {
    rest.split('+').collect()
  };
  let key = if plus_key {
    "+".to_owned()
  } else {
    parts.pop().unwrap_or_default().to_owned()
  };
  let invalid = |why: &str| format!("keybinding {binding:?} {why}");
  if parts.iter().any(|part| part.is_empty()) || key.is_empty() {
    return Err(invalid("is malformed"));
  }
  for part in &parts {
    if !MODIFIERS.contains(part) {
      return Err(invalid(&format!(
        "has an unknown modifier {part:?} (expected {})",
        MODIFIERS.join(", ")
      )));
    }
  }
  let function_key = key
    .strip_prefix('f')
    .and_then(|number| number.parse::<u8>().ok())
    .is_some_and(|number| (1..=24).contains(&number));
  let single = key.chars().count() == 1 && !key.chars().all(char::is_whitespace);
  if !(single || function_key || NAMED_KEYS.contains(&key.as_str())) {
    return Err(invalid(&format!("has an unknown key {key:?}")));
  }
  let mut modifiers: Vec<&str> = MODIFIERS
    .iter()
    .copied()
    .filter(|modifier| parts.contains(modifier))
    .collect();
  modifiers.dedup();
  modifiers.push(&key);
  Ok(modifiers.join("+"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn normalizes_valid_bindings() {
    assert_eq!(normalize("Shift+Ctrl+P"), Ok("ctrl+shift+p".into()));
    assert_eq!(normalize("mod+k"), Ok("mod+k".into()));
    assert_eq!(normalize("alt+F12"), Ok("alt+f12".into()));
    assert_eq!(normalize("ctrl+space"), Ok("ctrl+space".into()));
    assert_eq!(normalize("mod++"), Ok("mod++".into()));
    assert_eq!(normalize("/"), Ok("/".into()));
  }

  #[test]
  fn normalizes_chords() {
    assert_eq!(normalize("Ctrl+K  Ctrl+0"), Ok("ctrl+k ctrl+0".into()));
    assert_eq!(
      normalize("mod+k shift+mod+["),
      Ok("mod+k mod+shift+[".into())
    );
    assert!(normalize("mod+k mod+k mod+k").is_err());
    assert!(normalize("mod+k banana").is_err());
  }

  #[test]
  fn rejects_unknown_modifiers_and_keys() {
    assert!(normalize("hyper+k").is_err());
    assert!(normalize("ctrl+").is_err());
    assert!(normalize("ctrl+banana").is_err());
    assert!(normalize("ctrl+f25").is_err());
    assert!(normalize("").is_err());
  }
}
