//! Extension configuration: the settings an extension declares in its
//! manifest, and the values the user sets in `settings.toml`
//! (`[extensions.settings."<id>"]`), checked against those declarations.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigType {
  Boolean,
  Integer,
  Number,
  String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
  Boolean(bool),
  Integer(i64),
  Number(f64),
  Text(String),
}

/// A declared setting.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigProperty {
  pub kind: ConfigType,
  pub default: ConfigValue,
  pub description: String,
  pub minimum: Option<f64>,
  pub maximum: Option<f64>,
  /// The allowed values of a string setting.
  pub choices: Option<Vec<String>>,
}

impl ConfigProperty {
  /// Converts `value` to this setting's type, or explains why it doesn't
  /// fit.
  pub fn check(&self, value: &toml::Value) -> Result<ConfigValue, String> {
    let checked = match (self.kind, value) {
      (ConfigType::Boolean, toml::Value::Boolean(b)) => ConfigValue::Boolean(*b),
      (ConfigType::Integer, toml::Value::Integer(i)) => ConfigValue::Integer(*i),
      (ConfigType::Number, toml::Value::Float(f)) => ConfigValue::Number(*f),
      #[allow(clippy::cast_precision_loss)]
      (ConfigType::Number, toml::Value::Integer(i)) => ConfigValue::Number(*i as f64),
      (ConfigType::String, toml::Value::String(s)) => ConfigValue::Text(s.clone()),
      (kind, other) => {
        return Err(format!(
          "must be {}, not {}",
          match kind {
            ConfigType::Boolean => "true or false",
            ConfigType::Integer => "a whole number",
            ConfigType::Number => "a number",
            ConfigType::String => "a string",
          },
          other.type_str()
        ));
      }
    };
    #[allow(clippy::cast_precision_loss)]
    let number = match checked {
      ConfigValue::Integer(i) => Some(i as f64),
      ConfigValue::Number(f) => Some(f),
      _ => None,
    };
    if let Some(number) = number {
      if !number.is_finite() {
        return Err("must be a finite number".into());
      }
      if let Some(minimum) = self.minimum.filter(|minimum| number < *minimum) {
        return Err(format!("must be at least {minimum}"));
      }
      if let Some(maximum) = self.maximum.filter(|maximum| number > *maximum) {
        return Err(format!("must be at most {maximum}"));
      }
    }
    if let (ConfigValue::Text(text), Some(choices)) = (&checked, &self.choices)
      && !choices.contains(text)
    {
      return Err(format!("must be one of {}", choices.join(", ")));
    }
    Ok(checked)
  }
}

/// The effective values of an extension's settings: the user's where valid,
/// the declared defaults otherwise. Also returns a message for each user
/// value that was ignored.
pub fn resolve(
  declared: &BTreeMap<String, ConfigProperty>,
  user: Option<&toml::Table>,
) -> (BTreeMap<String, ConfigValue>, Vec<String>) {
  let mut values: BTreeMap<String, ConfigValue> = declared
    .iter()
    .map(|(key, property)| (key.clone(), property.default.clone()))
    .collect();
  let mut errors = Vec::new();
  for (key, value) in user.into_iter().flatten() {
    match declared.get(key) {
      Some(property) => match property.check(value) {
        Ok(checked) => {
          values.insert(key.clone(), checked);
        }
        Err(error) => errors.push(format!("{key} {error}; using the default")),
      },
      None => errors.push(format!("{key} is not a setting of this extension")),
    }
  }
  (values, errors)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn integer(minimum: Option<f64>, maximum: Option<f64>) -> ConfigProperty {
    ConfigProperty {
      kind: ConfigType::Integer,
      default: ConfigValue::Integer(3),
      description: String::new(),
      minimum,
      maximum,
      choices: None,
    }
  }

  #[test]
  fn checks_types_ranges_and_choices() {
    let property = integer(Some(1.0), Some(10.0));
    assert_eq!(
      property.check(&toml::Value::Integer(5)),
      Ok(ConfigValue::Integer(5))
    );
    assert!(property.check(&toml::Value::Integer(11)).is_err());
    assert!(property.check(&toml::Value::String("5".into())).is_err());
    let choice = ConfigProperty {
      kind: ConfigType::String,
      default: ConfigValue::Text("a".into()),
      description: String::new(),
      minimum: None,
      maximum: None,
      choices: Some(vec!["a".into(), "b".into()]),
    };
    assert!(choice.check(&toml::Value::String("c".into())).is_err());
  }

  #[test]
  fn resolves_user_values_over_defaults_and_reports_bad_ones() {
    let declared = BTreeMap::from([
      ("limit".to_owned(), integer(Some(1.0), None)),
      ("other".to_owned(), integer(None, None)),
    ]);
    let user: toml::Table = toml::from_str("limit = 0\nother = 7\nextra = true").expect("toml");
    let (values, errors) = resolve(&declared, Some(&user));
    assert_eq!(values["limit"], ConfigValue::Integer(3));
    assert_eq!(values["other"], ConfigValue::Integer(7));
    assert_eq!(errors.len(), 2);
  }
}
