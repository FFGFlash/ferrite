//! Images: an example Ferrite extension with webview viewers.
//!
//! The manifest declares two viewers: `image` for PNG, JPEG, GIF, WebP,
//! BMP, ICO, and AVIF files, which opens them instead of their bytes, and
//! `svg`, which SVG files open in only when the user chooses it (they're
//! text people edit). Both show the same page, `media/viewer.html`, which
//! draws the picture from the file itself, with a toolbar to change its
//! background and size (`media/viewer.js`).
//!
//! The page asks for the picture's details with a message, which runs the
//! viewers' `on_message` command. It reads the file's header (`info`) and
//! answers with a line saying what the picture is, such as
//! `PNG · 1920 × 1080 pixels · 8-bit RGBA · 245.3 KiB`, and its size in
//! pixels: what "actual size" means for an SVG file, which browsers don't
//! give a size when it only has a `viewBox`.

mod info;

use ferrite_extension_api::types::{
  Activation, ApiError, CommandContext, DocumentChange, DocumentId, DocumentInfo, Webview,
};
use ferrite_extension_api::{Guest, export_extension, viewers, webviews};

use info::{ImageInfo, Source};

/// How much of a file is read at first: enough for every header but a
/// JPEG's, whose size can come after large metadata segments.
const HEAD_BYTES: u32 = 64 * 1024;

fn describe(error: ApiError) -> String {
  match error {
    ApiError::InvalidArgument(message)
    | ApiError::TooLarge(message)
    | ApiError::InvalidRange(message) => message,
    ApiError::PermissionDenied(_) | ApiError::UnknownDocument | ApiError::StaleVersion(_) => {
      "the file can't be read".into()
    }
  }
}

/// Reads the file open in a viewer, remembering its size.
struct Viewed<'a> {
  viewer: &'a str,
  path: &'a str,
  size: u64,
}

impl Source for Viewed<'_> {
  fn read(&mut self, offset: u64, length: u32) -> Vec<u8> {
    match viewers::read_viewed_file(self.viewer, self.path, offset, length) {
      Ok(read) => {
        self.size = read.size;
        read.bytes
      }
      Err(_) => Vec::new(),
    }
  }
}

/// What the page shows at the end of its toolbar.
fn summary(info: Option<&ImageInfo>, size: u64) -> String {
  let mut parts = Vec::new();
  if let Some(info) = info {
    parts.push(info.format.to_owned());
    if let Some((width, height)) = info.size {
      parts.push(format!("{width} × {height} pixels"));
    }
    parts.extend(info.details.iter().cloned());
  }
  parts.push(info::file_size(size));
  parts.join(" · ")
}

/// `text` as a JSON string.
fn json_string(text: &str) -> String {
  let mut json = String::from("\"");
  for c in text.chars() {
    match c {
      '"' => json.push_str("\\\""),
      '\\' => json.push_str("\\\\"),
      c if c < ' ' => json.push_str(&format!("\\u{:04x}", u32::from(c))),
      c => json.push(c),
    }
  }
  json.push('"');
  json
}

/// The answer to the page: `{"details": "...", "width": 1920, "height": 1080}`,
/// with `null` sizes when the header doesn't say.
fn details_message(info: Option<&ImageInfo>, size: u64) -> String {
  let (width, height) = info
    .and_then(|info| info.size)
    .map_or(("null".into(), "null".into()), |(width, height)| {
      (width.to_string(), height.to_string())
    });
  format!(
    "{{\"details\":{},\"width\":{width},\"height\":{height}}}",
    json_string(&summary(info, size))
  )
}

/// Answers a viewer page's request for its picture's details.
fn answer() -> Result<(), String> {
  let message = webviews::message().ok_or("no message to answer")?;
  let Webview::Viewer(viewed) = &message.source else {
    return Err("only the viewers' pages ask for details".into());
  };
  let mut file = Viewed {
    viewer: &viewed.viewer,
    path: &viewed.path,
    size: 0,
  };
  let head =
    viewers::read_viewed_file(&viewed.viewer, &viewed.path, 0, HEAD_BYTES).map_err(describe)?;
  file.size = head.size;
  let info = info::identify(&head.bytes, &mut file);
  // False when the tab was closed meanwhile: nothing to do.
  webviews::post_message(&message.source, &details_message(info.as_ref(), file.size))
    .map_err(describe)?;
  Ok(())
}

struct Images;

impl Guest for Images {
  fn activate(_context: Activation) -> Result<(), String> {
    Ok(())
  }

  fn deactivate() {}

  fn run_command(command: String, _context: CommandContext) -> Result<(), String> {
    match command.as_str() {
      "ferrite.images.details" => answer(),
      other => Err(format!("unknown command {other}")),
    }
  }

  fn document_opened(_document: DocumentInfo) {}
  fn document_changed(_change: DocumentChange) {}
  fn document_closed(_document: DocumentId) {}
}

export_extension!(Images);

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn summarizes_what_is_known() {
    let png = ImageInfo {
      format: "PNG",
      size: Some((1920, 1080)),
      details: vec!["8-bit RGBA".into()],
    };
    assert_eq!(
      summary(Some(&png), 251_187),
      "PNG · 1920 × 1080 pixels · 8-bit RGBA · 245.3 KiB"
    );
    assert_eq!(summary(None, 12), "12 bytes");
  }

  #[test]
  fn answers_with_the_details_and_size_as_json() {
    let svg = ImageInfo {
      format: "SVG",
      size: Some((64, 64)),
      details: Vec::new(),
    };
    assert_eq!(
      details_message(Some(&svg), 512),
      r#"{"details":"SVG · 64 × 64 pixels · 512 bytes","width":64,"height":64}"#
    );
    assert_eq!(
      details_message(None, 3),
      r#"{"details":"3 bytes","width":null,"height":null}"#
    );
    assert_eq!(json_string("a\"b\\c\nd"), r#""a\"b\\c\u000ad""#);
  }
}
