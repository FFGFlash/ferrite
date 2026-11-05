use std::borrow::Cow;

/// A line break sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineEnding {
  #[default]
  Lf,
  CrLf,
  Cr,
}

impl LineEnding {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Lf => "\n",
      Self::CrLf => "\r\n",
      Self::Cr => "\r",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Lf => "LF",
      Self::CrLf => "CRLF",
      Self::Cr => "CR",
    }
  }

  /// The most common line ending among the first breaks in `bytes`, or `None`
  /// if there are no line breaks. Ties prefer LF, then CRLF.
  pub fn detect(bytes: impl IntoIterator<Item = u8>) -> Option<Self> {
    const SAMPLE_BREAKS: usize = 1000;
    let (mut lf, mut crlf, mut cr) = (0usize, 0usize, 0usize);
    let mut bytes = bytes.into_iter().peekable();
    while lf + crlf + cr < SAMPLE_BREAKS {
      match bytes.next() {
        None => break,
        Some(b'\n') => lf += 1,
        Some(b'\r') => {
          if bytes.next_if_eq(&b'\n').is_some() {
            crlf += 1;
          } else {
            cr += 1;
          }
        }
        Some(_) => {}
      }
    }
    if lf + crlf + cr == 0 {
      None
    } else if lf >= crlf && lf >= cr {
      Some(Self::Lf)
    } else if crlf >= cr {
      Some(Self::CrLf)
    } else {
      Some(Self::Cr)
    }
  }
}

/// Rewrites every line break in `text` (LF, CRLF, or CR) as `ending`.
pub fn normalize_line_endings(text: &str, ending: LineEnding) -> Cow<'_, str> {
  let target = ending.as_str();
  let needs_change = match ending {
    LineEnding::Lf => text.contains('\r'),
    LineEnding::Cr => text.contains('\n'),
    LineEnding::CrLf => {
      let bytes = text.as_bytes();
      bytes.iter().enumerate().any(|(i, &b)| match b {
        b'\n' => i == 0 || bytes[i - 1] != b'\r',
        b'\r' => bytes.get(i + 1) != Some(&b'\n'),
        _ => false,
      })
    }
  };
  if !needs_change {
    return Cow::Borrowed(text);
  }

  let mut out = String::with_capacity(text.len() + text.len() / 16);
  let mut chars = text.chars().peekable();
  while let Some(c) = chars.next() {
    match c {
      '\r' => {
        chars.next_if_eq(&'\n');
        out.push_str(target);
      }
      '\n' => out.push_str(target),
      c => out.push(c),
    }
  }
  Cow::Owned(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn detects_the_majority_line_ending() {
    assert_eq!(
      LineEnding::detect(*b"a\r\nb\r\nc\n"),
      Some(LineEnding::CrLf)
    );
    assert_eq!(LineEnding::detect(*b"a\nb\rc\n"), Some(LineEnding::Lf));
    assert_eq!(LineEnding::detect(*b"a\rb\r"), Some(LineEnding::Cr));
    assert_eq!(LineEnding::detect(*b"no breaks"), None);
  }

  #[test]
  fn normalizes_mixed_line_endings() {
    let mixed = "a\r\nb\nc\rd";
    assert_eq!(normalize_line_endings(mixed, LineEnding::Lf), "a\nb\nc\nd");
    assert_eq!(
      normalize_line_endings(mixed, LineEnding::CrLf),
      "a\r\nb\r\nc\r\nd"
    );
    assert_eq!(normalize_line_endings(mixed, LineEnding::Cr), "a\rb\rc\rd");
  }

  #[test]
  fn leaves_already_normalized_text_borrowed() {
    assert!(matches!(
      normalize_line_endings("a\r\nb", LineEnding::CrLf),
      Cow::Borrowed(_)
    ));
    assert!(matches!(
      normalize_line_endings("plain", LineEnding::Lf),
      Cow::Borrowed(_)
    ));
  }
}
