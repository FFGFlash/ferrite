//! Text encodings: detecting a file's encoding when it's read, and encoding
//! text back when it's saved, so that text the user didn't change is
//! written back byte for byte.
//!
//! Detection, in order:
//!
//! 1. A byte order mark: UTF-8, UTF-16 LE, or UTF-16 BE.
//! 2. NUL bytes in the first 8 KiB mean a binary file, unless the start
//!    looks like UTF-16 without a byte order mark (every other byte NUL).
//! 3. Valid UTF-8.
//! 4. A legacy encoding guessed by `chardetng`, if the text decodes in it
//!    without errors and encodes back to the same bytes.
//! 5. Otherwise Windows-1252, which gives every byte a character of its
//!    own, so even a file that isn't really text survives a save unchanged.

use std::fmt;
use std::io::{self, Write};

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};

/// How much of a file is checked for NUL bytes, as find in files does.
const BINARY_SAMPLE_BYTES: usize = 8 * 1024;
/// How much of a file is checked for UTF-16 without a byte order mark.
const UTF16_SAMPLE_BYTES: usize = 512;
/// How much of a file the encoding detector reads.
const GUESS_SAMPLE_BYTES: usize = 1024 * 1024;
/// Encoded bytes are written in pieces of this size.
const WRITE_BUFFER_BYTES: usize = 64 * 1024;

/// The encodings offered when reopening or saving a file with another
/// encoding: an id (an encoding label, or `utf-8-bom`) and a label.
pub const ENCODING_CHOICES: &[(&str, &str)] = &[
  ("utf-8", "UTF-8"),
  ("utf-8-bom", "UTF-8 with BOM"),
  ("utf-16le", "UTF-16 LE"),
  ("utf-16be", "UTF-16 BE"),
  ("windows-1252", "Western (Windows 1252)"),
  ("iso-8859-15", "Western (ISO 8859-15)"),
  ("macintosh", "Western (Mac Roman)"),
  ("windows-1250", "Central European (Windows 1250)"),
  ("iso-8859-2", "Central European (ISO 8859-2)"),
  ("windows-1251", "Cyrillic (Windows 1251)"),
  ("koi8-r", "Cyrillic (KOI8-R)"),
  ("koi8-u", "Cyrillic (KOI8-U)"),
  ("iso-8859-5", "Cyrillic (ISO 8859-5)"),
  ("ibm866", "Cyrillic (CP 866)"),
  ("windows-1253", "Greek (Windows 1253)"),
  ("iso-8859-7", "Greek (ISO 8859-7)"),
  ("windows-1254", "Turkish (Windows 1254)"),
  ("windows-1255", "Hebrew (Windows 1255)"),
  ("iso-8859-8", "Hebrew (ISO 8859-8)"),
  ("windows-1256", "Arabic (Windows 1256)"),
  ("iso-8859-6", "Arabic (ISO 8859-6)"),
  ("windows-1257", "Baltic (Windows 1257)"),
  ("iso-8859-4", "Baltic (ISO 8859-4)"),
  ("iso-8859-13", "Baltic (ISO 8859-13)"),
  ("windows-1258", "Vietnamese (Windows 1258)"),
  ("windows-874", "Thai (Windows 874)"),
  ("shift_jis", "Japanese (Shift JIS)"),
  ("euc-jp", "Japanese (EUC-JP)"),
  ("iso-2022-jp", "Japanese (ISO-2022-JP)"),
  ("gbk", "Simplified Chinese (GBK)"),
  ("gb18030", "Simplified Chinese (GB 18030)"),
  ("big5", "Traditional Chinese (Big5)"),
  ("euc-kr", "Korean (EUC-KR)"),
];

/// How a file's text is stored: an encoding, and whether the file starts
/// with a byte order mark (only UTF-8 and UTF-16 have one).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TextEncoding {
  encoding: &'static Encoding,
  bom: bool,
}

impl Default for TextEncoding {
  fn default() -> Self {
    Self::UTF8
  }
}

impl fmt::Debug for TextEncoding {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.id())
  }
}

impl TextEncoding {
  /// UTF-8 without a byte order mark: new files, and most files.
  pub const UTF8: Self = Self {
    encoding: UTF_8,
    bom: false,
  };
  pub const UTF8_BOM: Self = Self {
    encoding: UTF_8,
    bom: true,
  };
  /// Decodes any bytes, each to a character of its own.
  const BYTES: Self = Self {
    encoding: WINDOWS_1252,
    bom: false,
  };

  /// The encoding with this id (see [`Self::id`]) or another encoding
  /// label, such as `latin1`. UTF-16 gets a byte order mark. `None` for
  /// unknown labels, and for encodings text can't be saved in.
  pub fn from_id(id: &str) -> Option<Self> {
    let id = id.trim().to_ascii_lowercase();
    if id == "utf-8-bom" {
      return Some(Self::UTF8_BOM);
    }
    let encoding = Encoding::for_label(id.as_bytes())?;
    // `replacement` decodes everything to U+FFFD, and `x-user-defined`
    // isn't an encoding anyone saves text in.
    if encoding == encoding_rs::REPLACEMENT || encoding == encoding_rs::X_USER_DEFINED {
      return None;
    }
    Some(Self {
      encoding,
      bom: encoding == UTF_16LE || encoding == UTF_16BE,
    })
  }

  /// A stable identifier: the encoding's lowercase name (`utf-8`,
  /// `windows-1252`, `shift_jis`), or `utf-8-bom` for UTF-8 with a byte
  /// order mark.
  pub fn id(&self) -> String {
    if self.encoding == UTF_8 && self.bom {
      "utf-8-bom".into()
    } else {
      self.encoding.name().to_ascii_lowercase()
    }
  }

  /// What the user sees, such as `UTF-8` or `Western (Windows 1252)`.
  pub fn label(&self) -> String {
    let id = self.id();
    ENCODING_CHOICES
      .iter()
      .find(|(choice, _)| *choice == id)
      .map_or_else(
        || self.encoding.name().to_owned(),
        |(_, label)| (*label).to_owned(),
      )
  }

  pub fn has_bom(&self) -> bool {
    self.bom
  }

  pub fn is_utf8(&self) -> bool {
    self.encoding == UTF_8
  }

  fn bom_bytes(&self) -> &'static [u8] {
    match self.encoding {
      _ if !self.bom => b"",
      encoding if encoding == UTF_8 => b"\xEF\xBB\xBF",
      encoding if encoding == UTF_16LE => b"\xFF\xFE",
      encoding if encoding == UTF_16BE => b"\xFE\xFF",
      _ => b"",
    }
  }

  /// Writes `chunks` of text in this encoding, after the byte order mark
  /// if it has one. Stops at the first character the encoding can't
  /// represent, having written part of the text.
  pub fn write<'a>(
    &self,
    chunks: impl IntoIterator<Item = &'a str>,
    out: &mut impl Write,
  ) -> Result<(), EncodeError> {
    out.write_all(self.bom_bytes())?;
    if self.encoding == UTF_8 {
      for chunk in chunks {
        out.write_all(chunk.as_bytes())?;
      }
      return Ok(());
    }
    let mut buffer = Vec::with_capacity(WRITE_BUFFER_BYTES);
    if self.encoding == UTF_16LE || self.encoding == UTF_16BE {
      let little = self.encoding == UTF_16LE;
      // Chunks are whole characters, so surrogate pairs aren't split.
      for chunk in chunks {
        for unit in chunk.encode_utf16() {
          buffer.extend(if little {
            unit.to_le_bytes()
          } else {
            unit.to_be_bytes()
          });
          if buffer.len() >= WRITE_BUFFER_BYTES {
            out.write_all(&buffer)?;
            buffer.clear();
          }
        }
      }
      out.write_all(&buffer)?;
      return Ok(());
    }
    buffer.resize(WRITE_BUFFER_BYTES, 0);
    let mut encoder = self.encoding.new_encoder();
    let mut encode = |mut text: &str, last: bool| -> Result<(), EncodeError> {
      loop {
        let (result, read, written) =
          encoder.encode_from_utf8_without_replacement(text, &mut buffer, last);
        out.write_all(&buffer[..written])?;
        text = &text[read..];
        match result {
          encoding_rs::EncoderResult::InputEmpty => return Ok(()),
          encoding_rs::EncoderResult::OutputFull => {}
          encoding_rs::EncoderResult::Unmappable(character) => {
            return Err(EncodeError::Unmappable(character));
          }
        }
      }
    };
    for chunk in chunks {
      encode(chunk, false)?;
    }
    // Ends any escape sequence (ISO-2022-JP).
    encode("", true)
  }

  /// `text` in this encoding, with the byte order mark if it has one, or
  /// the first character the encoding can't represent.
  pub fn encode(&self, text: &str) -> Result<Vec<u8>, char> {
    let mut bytes = Vec::with_capacity(text.len() + 3);
    match self.write([text], &mut bytes) {
      Ok(()) => Ok(bytes),
      Err(EncodeError::Unmappable(character)) => Err(character),
      // Writing to a vector doesn't fail.
      Err(EncodeError::Io(_)) => Ok(bytes),
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
  #[error("the encoding can't represent {0:?}")]
  Unmappable(char),
  #[error(transparent)]
  Io(#[from] io::Error),
}

/// How to read a file's text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecodeOptions {
  /// Read it in this encoding instead of detecting one.
  pub encoding: Option<TextEncoding>,
  /// Read a file that looks binary (it has NUL bytes) instead of refusing.
  pub binary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
  /// The file looks binary.
  Binary,
  /// The bytes aren't valid in the encoding asked for.
  Invalid(TextEncoding),
  /// Encoding the text back wouldn't give the same bytes, so saving would
  /// change the file where the user didn't edit it.
  Lossy(TextEncoding),
}

/// Decodes a file's bytes, detecting their encoding unless `options` names
/// one. A byte order mark is removed from the text and recorded in the
/// encoding.
pub fn decode(
  bytes: Vec<u8>,
  options: DecodeOptions,
) -> Result<(String, TextEncoding), DecodeError> {
  if let Some(encoding) = options.encoding {
    return decode_as(bytes, encoding).map_err(|(error, _)| error);
  }
  let mut bytes = bytes;
  if let Some((encoding, _)) = Encoding::for_bom(&bytes) {
    match decode_as(
      bytes,
      TextEncoding {
        encoding,
        bom: true,
      },
    ) {
      Ok(decoded) => return Ok(decoded),
      Err((_, returned)) => bytes = returned,
    }
  }

  let sample = &bytes[..bytes.len().min(BINARY_SAMPLE_BYTES)];
  if sample.contains(&0) {
    if let Some(encoding) = utf16_without_bom(&bytes[..bytes.len().min(UTF16_SAMPLE_BYTES)]) {
      match decode_as(bytes, encoding) {
        Ok(decoded) => return Ok(decoded),
        Err((_, returned)) => bytes = returned,
      }
    }
    if !options.binary {
      return Err(DecodeError::Binary);
    }
  }

  let mut bytes = match String::from_utf8(bytes) {
    Ok(text) => return Ok((text, TextEncoding::UTF8)),
    Err(error) => error.into_bytes(),
  };

  // A file that looked binary is shown byte for byte, not as a guess.
  if !options.binary {
    let guessed = guess(&bytes);
    if guessed != UTF_8 {
      match decode_as(
        bytes,
        TextEncoding {
          encoding: guessed,
          bom: false,
        },
      ) {
        Ok(decoded) => return Ok(decoded),
        Err((_, returned)) => bytes = returned,
      }
    }
  }
  // Every byte decodes, and encodes back to itself.
  let (text, _) = WINDOWS_1252.decode_without_bom_handling(&bytes);
  Ok((text.into_owned(), TextEncoding::BYTES))
}

/// The legacy encoding `chardetng` guesses for the start of a file.
fn guess(bytes: &[u8]) -> &'static Encoding {
  let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
  let sample = &bytes[..bytes.len().min(GUESS_SAMPLE_BYTES)];
  detector.feed(sample, sample.len() == bytes.len());
  detector.guess(None, chardetng::Utf8Detection::Deny)
}

/// UTF-16 without a byte order mark, as VS Code detects it: text whose
/// every other byte is NUL (ASCII text in UTF-16).
fn utf16_without_bom(sample: &[u8]) -> Option<TextEncoding> {
  if sample.len() < 2 {
    return None;
  }
  let (pairs, _) = sample.as_chunks::<2>();
  let little = pairs.iter().all(|&[low, high]| low != 0 && high == 0);
  let big = pairs.iter().all(|&[high, low]| high == 0 && low != 0);
  let encoding = match (little, big) {
    (true, _) => UTF_16LE,
    (_, true) => UTF_16BE,
    _ => return None,
  };
  Some(TextEncoding {
    encoding,
    bom: false,
  })
}

/// Decodes `bytes` in `encoding`, without replacing anything. A byte order
/// mark matching a UTF-8 or UTF-16 encoding is removed and recorded. On
/// failure, the bytes are handed back as they were, so the caller can try
/// something else without having copied them.
fn decode_as(
  mut bytes: Vec<u8>,
  encoding: TextEncoding,
) -> Result<(String, TextEncoding), (DecodeError, Vec<u8>)> {
  let unicode = [UTF_8, UTF_16LE, UTF_16BE].contains(&encoding.encoding);
  let bom = TextEncoding {
    encoding: encoding.encoding,
    bom: unicode,
  }
  .bom_bytes();
  let has_bom = !bom.is_empty() && bytes.starts_with(bom);
  if has_bom {
    bytes.drain(..bom.len());
  }
  let encoding = TextEncoding {
    encoding: encoding.encoding,
    bom: has_bom,
  };
  let fail = |error, mut bytes: Vec<u8>| {
    if has_bom {
      bytes.splice(0..0, bom.iter().copied());
    }
    Err((error, bytes))
  };
  if encoding.encoding == UTF_8 {
    return match String::from_utf8(bytes) {
      Ok(text) => Ok((text, encoding)),
      Err(error) => fail(DecodeError::Invalid(encoding), error.into_bytes()),
    };
  }
  let Some(text) = encoding
    .encoding
    .decode_without_bom_handling_and_without_replacement(&bytes)
    .map(std::borrow::Cow::into_owned)
  else {
    return fail(DecodeError::Invalid(encoding), bytes);
  };
  // UTF-16 always round-trips; legacy encodings with several byte
  // sequences for one character (Shift JIS) or with escape sequences
  // (ISO-2022-JP) may not.
  if !unicode {
    let mut encoded = Vec::with_capacity(bytes.len());
    let same = encoding.write([text.as_str()], &mut encoded).is_ok() && encoded == bytes;
    if !same {
      return fail(DecodeError::Lossy(encoding), bytes);
    }
  }
  Ok((text, encoding))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn detect(bytes: &[u8]) -> Result<(String, TextEncoding), DecodeError> {
    decode(bytes.to_vec(), DecodeOptions::default())
  }

  fn id(bytes: &[u8]) -> String {
    detect(bytes).expect("decodes").1.id()
  }

  #[test]
  fn ids_and_labels_round_trip() {
    for (choice, label) in ENCODING_CHOICES {
      let encoding = TextEncoding::from_id(choice).expect(choice);
      assert_eq!(encoding.id(), *choice);
      assert_eq!(encoding.label(), *label);
    }
    assert_eq!(
      TextEncoding::from_id("latin1").map(|e| e.id()).as_deref(),
      Some("windows-1252")
    );
    assert_eq!(TextEncoding::from_id("UTF-8"), Some(TextEncoding::UTF8));
    assert_eq!(TextEncoding::from_id("replacement"), None);
    assert_eq!(TextEncoding::from_id("klingon"), None);
  }

  #[test]
  fn detects_unicode_by_byte_order_mark_or_content() {
    assert_eq!(detect(b"plain"), Ok(("plain".into(), TextEncoding::UTF8)));
    assert_eq!(
      detect("héllo".as_bytes()).map(|d| d.1),
      Ok(TextEncoding::UTF8)
    );
    assert_eq!(
      detect(b"\xEF\xBB\xBFhi"),
      Ok(("hi".into(), TextEncoding::UTF8_BOM))
    );
    assert_eq!(detect(b""), Ok((String::new(), TextEncoding::UTF8)));

    let (text, encoding) = detect(b"\xFF\xFEh\0\xE9\0").expect("decodes");
    assert_eq!((text.as_str(), encoding.id().as_str()), ("hé", "utf-16le"));
    assert!(encoding.has_bom());
    let (text, encoding) = detect(b"\xFE\xFF\0h\0\xE9").expect("decodes");
    assert_eq!((text.as_str(), encoding.id().as_str()), ("hé", "utf-16be"));

    // Without a byte order mark: every other byte is NUL.
    let (text, encoding) = detect(b"a\0b\0\r\0\n\0").expect("decodes");
    assert_eq!(
      (text.as_str(), encoding.id().as_str()),
      ("ab\r\n", "utf-16le")
    );
    assert!(!encoding.has_bom());
    assert_eq!(id(b"\0a\0b"), "utf-16be");
  }

  #[test]
  fn nul_bytes_mean_binary_unless_asked_to_read_anyway() {
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10";
    assert_eq!(detect(png), Err(DecodeError::Binary));
    assert_eq!(detect(b"text\0more"), Err(DecodeError::Binary));

    let anyway = DecodeOptions {
      binary: true,
      ..DecodeOptions::default()
    };
    // Valid UTF-8 stays UTF-8; anything else is read byte for byte.
    assert_eq!(
      decode(b"text\0more".to_vec(), anyway),
      Ok(("text\0more".into(), TextEncoding::UTF8))
    );
    let (text, encoding) = decode(png.to_vec(), anyway).expect("decodes");
    assert_eq!(encoding.id(), "windows-1252");
    assert_eq!(encoding.encode(&text), Ok(png.to_vec()));
  }

  #[test]
  fn every_byte_survives_the_fallback() {
    let bytes: Vec<u8> = (0..=255).collect();
    let (text, encoding) = decode(
      bytes.clone(),
      DecodeOptions {
        binary: true,
        ..DecodeOptions::default()
      },
    )
    .expect("decodes");
    assert_eq!(encoding, TextEncoding::BYTES);
    assert_eq!(text.chars().count(), 256);
    assert_eq!(encoding.encode(&text), Ok(bytes));
  }

  #[test]
  fn guesses_legacy_encodings() {
    // "café crème" in Windows-1252.
    let (text, encoding) = detect(b"caf\xE9 cr\xE8me, d\xE9j\xE0 vu").expect("decodes");
    assert_eq!(text, "café crème, déjà vu");
    assert_eq!(encoding.id(), "windows-1252");

    let russian = "Съешь же ещё этих мягких французских булок, да выпей чаю.";
    let (bytes, _, _) = encoding_rs::WINDOWS_1251.encode(russian);
    let (text, encoding) = detect(&bytes).expect("decodes");
    assert_eq!(
      (text.as_str(), encoding.id().as_str()),
      (russian, "windows-1251")
    );

    let japanese = "いろはにほへと ちりぬるを わかよたれそ つねならむ。日本語のテキストです。";
    let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(japanese);
    let (text, encoding) = detect(&bytes).expect("decodes");
    assert_eq!(
      (text.as_str(), encoding.id().as_str()),
      (japanese, "shift_jis")
    );
  }

  #[test]
  fn reads_in_a_chosen_encoding_only_if_valid_and_lossless() {
    let latin1 = TextEncoding::from_id("windows-1252").expect("known");
    let as_latin1 = |bytes: &[u8]| {
      decode(
        bytes.to_vec(),
        DecodeOptions {
          encoding: Some(latin1),
          binary: false,
        },
      )
    };
    // UTF-8 bytes read as Windows-1252, as the user asked.
    assert_eq!(as_latin1("é".as_bytes()), Ok(("Ã©".into(), latin1)));

    let utf8 = DecodeOptions {
      encoding: Some(TextEncoding::UTF8),
      binary: false,
    };
    assert_eq!(
      decode(b"caf\xE9".to_vec(), utf8),
      Err(DecodeError::Invalid(TextEncoding::UTF8))
    );
    // A byte order mark is kept whichever UTF-8 was asked for.
    assert_eq!(
      decode(b"\xEF\xBB\xBFx".to_vec(), utf8),
      Ok(("x".into(), TextEncoding::UTF8_BOM))
    );

    // 0xED 0x40 is an NEC-selected IBM extension character, which Shift JIS
    // encodes back as 0xFA 0x5C: saving would change the file.
    let shift_jis = TextEncoding::from_id("shift_jis").expect("known");
    assert_eq!(
      decode(
        b"\xED\x40".to_vec(),
        DecodeOptions {
          encoding: Some(shift_jis),
          binary: false,
        }
      ),
      Err(DecodeError::Lossy(shift_jis))
    );
  }

  #[test]
  fn encodes_text_back_to_the_same_bytes() {
    for bytes in [
      b"\xEF\xBB\xBFa\r\nb".to_vec(),
      b"\xFF\xFEa\0\r\0\n\0=\xD8\0\xDE".to_vec(),
      b"\xFE\xFF\0a\0\n".to_vec(),
      b"a\0b\0".to_vec(),
      b"caf\xE9 cr\xE8me, d\xE9j\xE0 vu".to_vec(),
    ] {
      let (text, encoding) = detect(&bytes).expect("decodes");
      assert_eq!(encoding.encode(&text), Ok(bytes), "{encoding:?}");
    }
  }

  #[test]
  fn writes_across_chunks_and_reports_unmappable_characters() {
    let shift_jis = TextEncoding::from_id("shift_jis").expect("known");
    let mut out = Vec::new();
    shift_jis.write(["日本", "語"], &mut out).expect("writes");
    assert_eq!(out, encoding_rs::SHIFT_JIS.encode("日本語").0.into_owned());

    let iso_2022_jp = TextEncoding::from_id("iso-2022-jp").expect("known");
    let encoded = iso_2022_jp.encode("日本 x").expect("encodes");
    // Ends in ASCII mode.
    assert!(encoded.ends_with(b" x"));
    assert_eq!(
      decode(
        encoded,
        DecodeOptions {
          encoding: Some(iso_2022_jp),
          binary: false
        }
      )
      .map(|d| d.0),
      Ok("日本 x".into())
    );

    let latin1 = TextEncoding::from_id("windows-1252").expect("known");
    assert_eq!(
      latin1.encode("price: 5€, or ¥"),
      Ok(b"price: 5\x80, or \xA5".to_vec())
    );
    assert_eq!(latin1.encode("snow ☃"), Err('☃'));
    assert_eq!(
      TextEncoding::UTF8.encode("snow ☃"),
      Ok("snow ☃".as_bytes().to_vec())
    );
  }
}
