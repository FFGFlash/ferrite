//! Markdown: an example Ferrite extension with a preview.
//!
//! Highlighting needs no code: the manifest declares the Tree-sitter
//! grammars and their queries. This code is the preview:
//!
//! - **Open Preview** (Ctrl+Shift+V) shows the active Markdown document,
//!   rendered, in a tab of its own.
//! - **Open Preview to the Side** (Ctrl+K V) shows it beside the editor.
//!
//! A preview follows its document as it changes, and closes with it. The
//! page is sent as preview nodes (headings, paragraphs, lists, tables,
//! links, pictures), which the workbench draws itself; see `render` for how
//! Markdown maps to them.

mod render;

use std::cell::RefCell;
use std::collections::BTreeSet;

use ferrite_extension_api::types::{
  Activation, ApiError, CommandContext, DocumentChange, DocumentId, DocumentInfo, LogLevel,
  PreviewOptions,
};
use ferrite_extension_api::{Guest, export_extension, host, types, workbench};

/// The preview declared in the manifest. Each document has its own, keyed
/// by the document's id.
const PREVIEW: &str = "preview";
const CHUNK_LINES: u32 = 10_000;

thread_local! {
  /// Documents whose previews are open.
  static PREVIEWED: RefCell<BTreeSet<DocumentId>> = const { RefCell::new(BTreeSet::new()) };
}

fn describe(error: &ApiError) -> String {
  match error {
    ApiError::PermissionDenied(_) => {
      "Markdown can't read documents; allow it in the Extensions view".into()
    }
    ApiError::UnknownDocument => "the document is no longer open".into(),
    ApiError::StaleVersion(_) => "the document changed; try again".into(),
    ApiError::InvalidRange(message)
    | ApiError::TooLarge(message)
    | ApiError::InvalidArgument(message) => message.clone(),
  }
}

/// The document's file name, for the preview's title.
fn file_name(info: &DocumentInfo) -> &str {
  info
    .path
    .as_deref()
    .and_then(|path| path.rsplit(['/', '\\']).next())
    .unwrap_or("Untitled")
}

fn read_text(info: &DocumentInfo) -> Result<String, ApiError> {
  let mut text = String::with_capacity(usize::try_from(info.length).unwrap_or(0));
  let mut start = 0;
  while start < info.line_count {
    let end = start.saturating_add(CHUNK_LINES).min(info.line_count);
    let slice = host::read_lines(info.id, start, end)?;
    if slice.lines.is_empty() {
      break;
    }
    start += u32::try_from(slice.lines.len()).unwrap_or(u32::MAX);
    for line in slice.lines {
      text.push_str(&line);
      text.push('\n');
    }
  }
  Ok(text)
}

/// Renders a document into its preview, and forgets the preview if the
/// user has closed it.
fn update(id: DocumentId) -> Result<(), ApiError> {
  let info = host::document(id)?;
  let nodes = render::render(&read_text(&info)?);
  if !workbench::set_preview_content(PREVIEW, &id.to_string(), &nodes)? {
    PREVIEWED.with_borrow_mut(|previewed| previewed.remove(&id));
  }
  Ok(())
}

struct Markdown;

impl Guest for Markdown {
  fn activate(_context: Activation) -> Result<(), String> {
    Ok(())
  }

  fn deactivate() {}

  fn run_command(command: String, context: CommandContext) -> Result<(), String> {
    let beside = match command.as_str() {
      "ferrite.markdown.preview" => false,
      "ferrite.markdown.previewToSide" => true,
      other => return Err(format!("unknown command {other}")),
    };
    let Some(id) = context.document else {
      return Err("Open a Markdown file to preview it.".into());
    };
    let info = host::document(id).map_err(|error| describe(&error))?;
    if info.language.as_deref() != Some("markdown") {
      return Err(format!("{} isn't a Markdown file.", file_name(&info)));
    }
    let options = PreviewOptions {
      title: format!("Preview {}", file_name(&info)),
      document: Some(id),
      beside,
    };
    workbench::show_preview(PREVIEW, &id.to_string(), &options)
      .map_err(|error| describe(&error))?;
    PREVIEWED.with_borrow_mut(|previewed| previewed.insert(id));
    update(id).map_err(|error| describe(&error))
  }

  fn document_opened(_document: DocumentInfo) {}

  fn document_changed(change: DocumentChange) {
    let id = change.document;
    if !PREVIEWED.with_borrow(|previewed| previewed.contains(&id)) {
      return;
    }
    if let Err(error) = update(id) {
      host::log(
        LogLevel::Warn,
        &format!("updating the preview failed: {}", describe(&error)),
      );
    }
  }

  fn document_closed(document: DocumentId) {
    // The editor closes the document's preview itself.
    PREVIEWED.with_borrow_mut(|previewed| previewed.remove(&document));
  }
}

export_extension!(Markdown);
