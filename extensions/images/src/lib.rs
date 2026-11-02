//! Images: an example Ferrite extension with viewers.
//!
//! The manifest declares two viewers: `image` for PNG, JPEG, GIF, WebP,
//! BMP, ICO, and AVIF files, which opens them instead of their bytes, and
//! `svg`, which SVG files open in only when the user chooses it (they're
//! text people edit). Each runs a command with the file's path as its
//! argument, when the file opens in it and when the file changes.
//!
//! The command reads the file's header (`info`), and fills the viewer in
//! with the picture, which the workbench draws from the file, and a line
//! saying what it is: `PNG · 1920 × 1080 pixels · 8-bit RGBA · 245.3 KiB`.

mod info;

use ferrite_extension_api::types::{
  Activation, ApiError, CommandContext, DocumentChange, DocumentId, DocumentInfo, PreviewElement,
  PreviewImage, PreviewNode,
};
use ferrite_extension_api::{Guest, export_extension, viewers, workbench};

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

/// A file name as a relative link to the file, which the host resolves
/// from the file's folder: escaped where it would otherwise end the path
/// (`#`, `?`), be decoded (`%`), or be trimmed (spaces), and after `./`, so
/// a name like `a:b.png` isn't taken for a web address.
fn link_to(name: &str) -> String {
  let mut link = String::from("./");
  for c in name.chars() {
    match c {
      '%' => link.push_str("%25"),
      '#' => link.push_str("%23"),
      '?' => link.push_str("%3F"),
      ' ' => link.push_str("%20"),
      c => link.push(c),
    }
  }
  link
}

/// What the line under the picture says.
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

fn show(viewer: &str, path: &str) -> Result<(), String> {
  let mut file = Viewed {
    viewer,
    path,
    size: 0,
  };
  let head = viewers::read_viewed_file(viewer, path, 0, HEAD_BYTES).map_err(describe)?;
  file.size = head.size;
  let info = info::identify(&head.bytes, &mut file);
  let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
  let node = |parent, element| PreviewNode { parent, element };
  let nodes = vec![
    node(None, PreviewElement::Paragraph),
    node(
      Some(0),
      PreviewElement::Image(PreviewImage {
        source: link_to(name),
        description: name.to_owned(),
        title: None,
      }),
    ),
    node(None, PreviewElement::Paragraph),
    node(
      Some(2),
      PreviewElement::Text(summary(info.as_ref(), file.size)),
    ),
  ];
  // False when the tab was closed meanwhile: nothing to do.
  viewers::set_viewer_content(viewer, path, &nodes).map_err(describe)?;
  Ok(())
}

struct Images;

impl Guest for Images {
  fn activate(_context: Activation) -> Result<(), String> {
    Ok(())
  }

  fn deactivate() {}

  fn run_command(command: String, _context: CommandContext) -> Result<(), String> {
    let viewer = match command.as_str() {
      "example.images.show" => "image",
      "example.images.showSvg" => "svg",
      other => return Err(format!("unknown command {other}")),
    };
    let path = workbench::command_argument().ok_or("no file to show")?;
    show(viewer, &path)
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
  fn links_to_awkward_file_names() {
    assert_eq!(link_to("cat.png"), "./cat.png");
    assert_eq!(link_to("100% #1 a?.png"), "./100%25%20%231%20a%3F.png");
    assert_eq!(link_to("a:b.png"), "./a:b.png");
  }

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
}
