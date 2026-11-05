//! Decoding terminal output, which arrives in chunks that can end in the
//! middle of a character.

/// Decodes a stream of bytes as UTF-8. A character split across chunks is
/// held back until the rest arrives; invalid bytes become U+FFFD.
#[derive(Debug, Default)]
pub struct Utf8Stream {
  /// The start of a character whose remaining bytes haven't arrived.
  partial: Vec<u8>,
}

impl Utf8Stream {
  /// Decodes `bytes` after any partial character from the previous chunk.
  pub fn decode(&mut self, bytes: &[u8]) -> String {
    let mut input = std::mem::take(&mut self.partial);
    input.extend_from_slice(bytes);
    let mut output = String::with_capacity(input.len());
    let mut rest = input.as_slice();
    loop {
      match std::str::from_utf8(rest) {
        Ok(text) => {
          output.push_str(text);
          break;
        }
        Err(error) => {
          let (valid, after) = rest.split_at(error.valid_up_to());
          // The prefix was just validated, so this never replaces anything.
          output.push_str(&String::from_utf8_lossy(valid));
          match error.error_len() {
            Some(invalid) => {
              output.push(char::REPLACEMENT_CHARACTER);
              rest = &after[invalid..];
            }
            // An incomplete character at the end: wait for the rest.
            None => {
              self.partial = after.to_vec();
              break;
            }
          }
        }
      }
    }
    output
  }

  /// Ends the stream; an unfinished character becomes U+FFFD.
  pub fn finish(&mut self) -> String {
    if self.partial.is_empty() {
      String::new()
    } else {
      self.partial.clear();
      char::REPLACEMENT_CHARACTER.to_string()
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn passes_complete_text_through() {
    let mut stream = Utf8Stream::default();
    assert_eq!(
      stream.decode(b"hello \x1b[1mworld\x1b[0m"),
      "hello \x1b[1mworld\x1b[0m"
    );
    assert_eq!(stream.finish(), "");
  }

  #[test]
  fn joins_characters_split_across_chunks() {
    let text = "a€b😀c";
    let bytes = text.as_bytes();
    // Every split point, including inside the 3- and 4-byte characters.
    for split in 0..=bytes.len() {
      let mut stream = Utf8Stream::default();
      let mut output = stream.decode(&bytes[..split]);
      output.push_str(&stream.decode(&bytes[split..]));
      assert_eq!(output, text, "split at {split}");
    }
    // One byte at a time.
    let mut stream = Utf8Stream::default();
    let output: String = bytes.iter().map(|byte| stream.decode(&[*byte])).collect();
    assert_eq!(output, text);
  }

  #[test]
  fn replaces_invalid_bytes() {
    let mut stream = Utf8Stream::default();
    assert_eq!(stream.decode(b"a\xffb\xc3(c"), "a\u{fffd}b\u{fffd}(c");
  }

  #[test]
  fn an_unfinished_character_at_the_end_is_replaced() {
    let mut stream = Utf8Stream::default();
    assert_eq!(stream.decode(b"ok\xe2\x82"), "ok");
    assert_eq!(stream.finish(), "\u{fffd}");
    assert_eq!(stream.finish(), "");
  }
}
