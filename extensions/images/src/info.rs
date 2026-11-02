//! What an image file is: its format and size in pixels, read from its
//! header. Formats are told apart by their first bytes, not the file name.
//!
//! This is plain Rust with no host calls, so it's tested on the host
//! target (`cargo test -p images --target <your host triple>`).

/// Reads bytes of a file: up to `length` from `offset`, fewer at its end.
pub trait Source {
  fn read(&mut self, offset: u64, length: u32) -> Vec<u8>;
}

impl Source for &[u8] {
  fn read(&mut self, offset: u64, length: u32) -> Vec<u8> {
    let start = usize::try_from(offset)
      .unwrap_or(usize::MAX)
      .min(self.len());
    let end = start.saturating_add(length as usize).min(self.len());
    self[start..end].to_vec()
  }
}

/// How many segments or chunks are walked looking for the size, at most.
const MAX_STEPS: usize = 2_000;
/// How much of an SVG file is searched for its size.
const SVG_HEAD_BYTES: u32 = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
  pub format: &'static str,
  /// Width and height in pixels, if the header says.
  pub size: Option<(u64, u64)>,
  /// More about it, such as `8-bit RGBA` or `animated`.
  pub details: Vec<String>,
}

impl ImageInfo {
  fn new(format: &'static str, size: Option<(u64, u64)>) -> Self {
    Self {
      format,
      size,
      details: Vec::new(),
    }
  }
}

fn u16_be(bytes: &[u8], at: usize) -> Option<u64> {
  Some(u64::from(u16::from_be_bytes(
    bytes.get(at..at + 2)?.try_into().ok()?,
  )))
}

fn u16_le(bytes: &[u8], at: usize) -> Option<u64> {
  Some(u64::from(u16::from_le_bytes(
    bytes.get(at..at + 2)?.try_into().ok()?,
  )))
}

fn u24_le(bytes: &[u8], at: usize) -> Option<u64> {
  let b = bytes.get(at..at + 3)?;
  Some(u64::from(b[0]) | u64::from(b[1]) << 8 | u64::from(b[2]) << 16)
}

fn u32_be(bytes: &[u8], at: usize) -> Option<u64> {
  Some(u64::from(u32::from_be_bytes(
    bytes.get(at..at + 4)?.try_into().ok()?,
  )))
}

fn u32_le(bytes: &[u8], at: usize) -> Option<u64> {
  Some(u64::from(u32::from_le_bytes(
    bytes.get(at..at + 4)?.try_into().ok()?,
  )))
}

fn i32_le(bytes: &[u8], at: usize) -> Option<i64> {
  Some(i64::from(i32::from_le_bytes(
    bytes.get(at..at + 4)?.try_into().ok()?,
  )))
}

/// What the file is, from `head` (its first bytes) and, for formats whose
/// size can be further in, more reads from `source`. `None` if it isn't an
/// image this knows.
pub fn identify(head: &[u8], source: &mut dyn Source) -> Option<ImageInfo> {
  if head.starts_with(b"\x89PNG\r\n\x1a\n") {
    Some(png(head, source))
  } else if head.starts_with(b"\xFF\xD8\xFF") {
    Some(jpeg(source))
  } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
    Some(ImageInfo::new("GIF", u16_le(head, 6).zip(u16_le(head, 8))))
  } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
    Some(webp(head))
  } else if head.starts_with(b"BM") {
    Some(bmp(head))
  } else if head.get(..4) == Some(b"\0\0\x01\0") || head.get(..4) == Some(b"\0\0\x02\0") {
    Some(ico(head))
  } else if head.get(4..8) == Some(b"ftyp") && matches!(head.get(8..12), Some(b"avif" | b"avis")) {
    Some(avif(head))
  } else {
    svg(&source.read(0, SVG_HEAD_BYTES))
  }
}

fn png(head: &[u8], source: &mut dyn Source) -> ImageInfo {
  // The first chunk is IHDR.
  if head.get(12..16) != Some(b"IHDR") {
    return ImageInfo::new("PNG", None);
  }
  let mut info = ImageInfo::new("PNG", u32_be(head, 16).zip(u32_be(head, 20)));
  if let (Some(&depth), Some(&color)) = (head.get(24), head.get(25)) {
    let kind = match color {
      0 => "grayscale",
      2 => "RGB",
      3 => "indexed color",
      4 => "grayscale with alpha",
      6 => "RGBA",
      _ => "unknown color type",
    };
    info.details.push(format!("{depth}-bit {kind}"));
  }
  if head.get(28) == Some(&1) {
    info.details.push("interlaced".into());
  }
  // An animated PNG says so in an acTL chunk before the image data.
  let mut offset = 8u64;
  for _ in 0..MAX_STEPS {
    let header = source.read(offset, 16);
    let (Some(length), Some(kind)) = (u32_be(&header, 0), header.get(4..8)) else {
      break;
    };
    match kind {
      b"acTL" => {
        info.format = "APNG";
        if let Some(frames) = u32_be(&header, 8) {
          info.details.push(format!("animated, {frames} frames"));
        }
        break;
      }
      b"IDAT" | b"IEND" => break,
      _ => offset += 12 + length,
    }
  }
  info
}

fn jpeg(source: &mut dyn Source) -> ImageInfo {
  let mut offset = 2u64;
  for _ in 0..MAX_STEPS {
    let header = source.read(offset, 10);
    if header.len() < 2 || header[0] != 0xFF {
      break;
    }
    let marker = header[1];
    // Padding, and markers without a length.
    if marker == 0xFF {
      offset += 1;
      continue;
    }
    if marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
      offset += 2;
      continue;
    }
    let Some(length) = u16_be(&header, 2) else {
      break;
    };
    // Start of frame: every SOFn but DHT (C4), JPG (C8), and DAC (CC).
    if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
      let mut info = ImageInfo::new("JPEG", u16_be(&header, 7).zip(u16_be(&header, 5)));
      if let Some(&components) = header.get(9) {
        info.details.push(match components {
          1 => "grayscale".into(),
          3 => "color".into(),
          4 => "CMYK".into(),
          n => format!("{n} components"),
        });
      }
      if matches!(marker, 0xC2 | 0xC6 | 0xCA | 0xCE) {
        info.details.push("progressive".into());
      }
      return info;
    }
    // Start of scan: the size should have come before.
    if marker == 0xDA {
      break;
    }
    offset += 2 + length;
  }
  ImageInfo::new("JPEG", None)
}

fn webp(head: &[u8]) -> ImageInfo {
  match head.get(12..16) {
    Some(b"VP8 ") => {
      let size = u16_le(head, 26)
        .zip(u16_le(head, 28))
        .map(|(w, h)| (w & 0x3FFF, h & 0x3FFF));
      let mut info = ImageInfo::new("WebP", size);
      info.details.push("lossy".into());
      info
    }
    Some(b"VP8L") => {
      let bits = u32_le(head, 21);
      let size = bits.map(|bits| ((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1));
      let mut info = ImageInfo::new("WebP", size);
      info.details.push("lossless".into());
      info
    }
    Some(b"VP8X") => {
      let flags = head.get(20).copied().unwrap_or(0);
      let size = u24_le(head, 24)
        .zip(u24_le(head, 27))
        .map(|(w, h)| (w + 1, h + 1));
      let mut info = ImageInfo::new("WebP", size);
      if flags & 0x02 != 0 {
        info.details.push("animated".into());
      }
      if flags & 0x10 != 0 {
        info.details.push("with alpha".into());
      }
      info
    }
    _ => ImageInfo::new("WebP", None),
  }
}

fn bmp(head: &[u8]) -> ImageInfo {
  let (size, depth) = match u32_le(head, 14) {
    // OS/2 bitmaps have a 12-byte header with 16-bit sizes.
    Some(12) => (u16_le(head, 18).zip(u16_le(head, 20)), u16_le(head, 24)),
    Some(_) => (
      i32_le(head, 18)
        .zip(i32_le(head, 22))
        // A negative height means the rows are stored top down.
        .map(|(w, h)| (w.unsigned_abs(), h.unsigned_abs())),
      u16_le(head, 28),
    ),
    None => (None, None),
  };
  let mut info = ImageInfo::new("BMP", size);
  if let Some(depth) = depth {
    info.details.push(format!("{depth}-bit"));
  }
  info
}

fn ico(head: &[u8]) -> ImageInfo {
  let format = if head[2] == 2 { "CUR" } else { "ICO" };
  let count = u16_le(head, 4).unwrap_or(0) as usize;
  // Each entry is 16 bytes; a width or height of 0 means 256.
  let mut sizes: Vec<(u64, u64)> = (0..count)
    .filter_map(|index| head.get(6 + index * 16..8 + index * 16))
    .map(|entry| {
      let side = |byte: u8| if byte == 0 { 256 } else { u64::from(byte) };
      (side(entry[0]), side(entry[1]))
    })
    .collect();
  sizes.sort_unstable();
  sizes.dedup();
  let mut info = ImageInfo::new(format, sizes.last().copied());
  if sizes.len() > 1 {
    let list: Vec<String> = sizes.iter().map(|(w, h)| format!("{w} × {h}")).collect();
    info
      .details
      .push(format!("{} sizes: {}", sizes.len(), list.join(", ")));
  }
  info
}

fn avif(head: &[u8]) -> ImageInfo {
  // The image spatial extents property: version and flags, width, height.
  let size = head
    .windows(4)
    .position(|window| window == b"ispe")
    .and_then(|at| u32_be(head, at + 8).zip(u32_be(head, at + 12)));
  ImageInfo::new("AVIF", size)
}

/// An SVG file's size from its root element's `width` and `height`, or
/// else its `viewBox`.
fn svg(head: &[u8]) -> Option<ImageInfo> {
  let text = String::from_utf8_lossy(head);
  let start = text.find("<svg")?;
  let tag = &text[start..start + text[start..].find('>')?];
  let attribute = |name: &str| {
    let at = tag.find(&format!(" {name}="))? + name.len() + 2;
    let quote = tag[at..].chars().next()?;
    let value = &tag[at + 1..];
    Some(value[..value.find(quote)?].trim().to_owned())
  };
  // Only plain numbers or pixels; 100% says nothing about the size.
  let pixels = |value: String| -> Option<u64> {
    let number: f64 = value.strip_suffix("px").unwrap_or(&value).parse().ok()?;
    (number > 0.0).then(|| number.round() as u64)
  };
  let size = attribute("width")
    .and_then(pixels)
    .zip(attribute("height").and_then(pixels))
    .or_else(|| {
      let view_box = attribute("viewBox")?;
      let parts: Vec<f64> = view_box
        .split([' ', ','])
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
      match parts[..] {
        [_, _, w, h] if w > 0.0 && h > 0.0 => Some((w.round() as u64, h.round() as u64)),
        _ => None,
      }
    });
  Some(ImageInfo::new("SVG", size))
}

/// A file size the way people read it: `512 bytes`, `245.3 KiB`, `1.2 MiB`.
pub fn file_size(bytes: u64) -> String {
  const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
  if bytes < 1024 {
    return format!("{bytes} {}", if bytes == 1 { "byte" } else { "bytes" });
  }
  let mut value = bytes as f64 / 1024.0;
  let mut unit = 0;
  while value >= 1024.0 && unit < UNITS.len() - 1 {
    value /= 1024.0;
    unit += 1;
  }
  format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
  use super::*;

  fn info(bytes: &[u8]) -> Option<ImageInfo> {
    identify(bytes, &mut &bytes[..])
  }

  fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(kind);
    chunk.extend_from_slice(data);
    chunk.extend_from_slice(&[0; 4]);
    chunk
  }

  #[test]
  fn reads_png_headers_and_animation() {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &[0, 0, 7, 128, 0, 0, 4, 56, 8, 6, 0, 0, 0]));
    let mut plain = png.clone();
    plain.extend(chunk(b"IDAT", b"..."));
    let found = info(&plain).expect("a PNG");
    assert_eq!(found.format, "PNG");
    assert_eq!(found.size, Some((1920, 1080)));
    assert_eq!(found.details, ["8-bit RGBA"]);

    png.extend(chunk(b"tEXt", b"Comment\0hi"));
    png.extend(chunk(b"acTL", &[0, 0, 0, 12, 0, 0, 0, 0]));
    let found = info(&png).expect("an APNG");
    assert_eq!(found.format, "APNG");
    assert_eq!(found.details, ["8-bit RGBA", "animated, 12 frames"]);
  }

  #[test]
  fn finds_the_jpeg_frame_after_other_segments() {
    let mut jpeg = b"\xFF\xD8".to_vec();
    // An APP1 segment (EXIF) of 300 bytes, then a progressive frame.
    jpeg.extend_from_slice(&[0xFF, 0xE1, 0x01, 0x2C]);
    jpeg.extend(std::iter::repeat_n(0, 298));
    jpeg.extend_from_slice(&[0xFF, 0xC2, 0, 17, 8, 0x02, 0x58, 0x03, 0x20, 3]);
    let found = info(&jpeg).expect("a JPEG");
    assert_eq!(found.size, Some((800, 600)));
    assert_eq!(found.details, ["color", "progressive"]);
  }

  #[test]
  fn reads_gif_webp_bmp_ico_and_avif_sizes() {
    assert_eq!(
      info(b"GIF89a\x40\x01\xF0\x00").and_then(|i| i.size),
      Some((320, 240))
    );

    let mut lossless = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2F".to_vec();
    // Width 100 and height 50, less one, in 14 bits each.
    lossless.extend_from_slice(&(99u32 | 49 << 14).to_le_bytes());
    let found = info(&lossless).expect("a WebP");
    assert_eq!(
      (found.size, found.details),
      (Some((100, 50)), vec!["lossless".to_owned()])
    );

    let mut extended = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\x12\0\0\0".to_vec();
    extended.extend_from_slice(&[199, 0, 0, 99, 0, 0]);
    let found = info(&extended).expect("a WebP");
    assert_eq!(found.size, Some((200, 100)));
    assert_eq!(found.details, ["animated", "with alpha"]);

    let mut bmp = b"BM".to_vec();
    bmp.extend_from_slice(&[0; 12]);
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&64i32.to_le_bytes());
    bmp.extend_from_slice(&(-32i32).to_le_bytes());
    bmp.extend_from_slice(&[1, 0, 24, 0]);
    let found = info(&bmp).expect("a BMP");
    assert_eq!(
      (found.size, found.details),
      (Some((64, 32)), vec!["24-bit".to_owned()])
    );

    let mut ico = b"\0\0\x01\0\x02\0".to_vec();
    ico.extend_from_slice(&[16, 16, 0, 0, 1, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    ico.extend_from_slice(&[0, 0, 0, 0, 1, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let found = info(&ico).expect("an icon");
    assert_eq!(found.size, Some((256, 256)));
    assert_eq!(found.details, ["2 sizes: 16 × 16, 256 × 256"]);

    let mut avif = b"\0\0\0\x1CftypavifX".to_vec();
    avif.extend_from_slice(b"\0\0\0\x14ispe\0\0\0\0\0\0\x04\0\0\0\x03\0");
    assert_eq!(info(&avif).and_then(|i| i.size), Some((1024, 768)));
  }

  #[test]
  fn reads_svg_sizes_from_attributes_or_the_view_box() {
    let sized =
      br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="48px" height='24'>"#;
    assert_eq!(info(sized).and_then(|i| i.size), Some((48, 24)));
    let boxed = br#"<svg viewBox="0 0 640 480" width="100%">"#;
    assert_eq!(info(boxed).and_then(|i| i.size), Some((640, 480)));
    assert_eq!(info(b"just text"), None);
  }

  #[test]
  fn truncated_headers_give_no_size_rather_than_failing() {
    assert_eq!(info(b"\x89PNG\r\n\x1a\n").map(|i| i.size), Some(None));
    assert_eq!(info(b"\xFF\xD8\xFF\xE0").map(|i| i.size), Some(None));
    assert_eq!(info(b"BM").map(|i| i.size), Some(None));
  }

  #[test]
  fn formats_file_sizes() {
    assert_eq!(file_size(1), "1 byte");
    assert_eq!(file_size(900), "900 bytes");
    assert_eq!(file_size(251_187), "245.3 KiB");
    assert_eq!(file_size(5 * 1024 * 1024), "5.0 MiB");
  }
}
