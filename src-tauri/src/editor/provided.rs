//! Read-only documents extensions provide for addresses with their schemes,
//! such as `git:HEAD:src/main.rs` (the extension API's `documents`
//! interface). The text comes from the extension when the document opens,
//! and again whenever the extension says it changed.

use ferrite_core::DocumentId;
use tauri::{AppHandle, Manager};

use super::EditorState;
use super::commands::show_hidden;
use super::protocol::DocumentSummary;
use crate::error::CommandError;
use crate::events::{self, AppEvent};
use crate::extensions::ExtensionsState;

/// Opens the document at a provided address, or finds it open. With
/// `hidden`, one not open yet opens for a diff tab only; without, a hidden
/// one gets listed.
#[tauri::command]
pub async fn document_open_provided(
  app: AppHandle,
  uri: String,
  hidden: Option<bool>,
) -> Result<DocumentSummary, CommandError> {
  let hidden = hidden.unwrap_or(false);
  let state = app.state::<EditorState>();
  let open = |state: &EditorState| {
    let editor = state.lock();
    let id = editor.find_by_uri(&uri)?;
    editor.document(id).ok().map(DocumentSummary::of)
  };
  let summary = match open(&state) {
    Some(summary) => summary,
    None => {
      let text = provide(&app, uri.clone()).await?;
      let mut editor = state.lock();
      // Another request may have opened it meanwhile.
      if let Some(id) = editor.find_by_uri(&uri) {
        DocumentSummary::of(editor.document(id)?)
      } else {
        let id = editor.open_provided(uri.clone(), &text);
        state.set_hidden(id, hidden);
        tracing::info!(document = %id, %uri, hidden, "opened a provided document");
        let document = editor.document(id)?;
        app.state::<ExtensionsState>().sync(document);
        DocumentSummary::of(document)
      }
    }
  };
  if !hidden {
    show_hidden(&app, DocumentId::from_raw(summary.id));
  }
  Ok(summary)
}

/// The text at `uri`, from its extension; empty when there's nothing there.
async fn provide(app: &AppHandle, uri: String) -> Result<String, CommandError> {
  let app = app.clone();
  tauri::async_runtime::spawn_blocking(move || {
    let extensions = app.state::<ExtensionsState>();
    let host = extensions
      .host()
      .ok_or_else(|| "Extensions aren't running.".to_owned())?;
    host.provide_text(&uri)
  })
  .await
  .map_err(CommandError::internal)?
  .map(Option::unwrap_or_default)
  .map_err(|message| CommandError::new("notProvided", message))
}

/// Whether `uri` is one of `patterns`: an address, or a prefix ending in `*`.
fn matches(patterns: &[String], uri: &str) -> bool {
  patterns
    .iter()
    .any(|pattern| match pattern.strip_suffix('*') {
      Some(prefix) => uri.starts_with(prefix),
      None => pattern == uri,
    })
}

/// Asks extensions again for the texts of open provided documents whose
/// addresses are among `patterns`, in the background.
pub fn refresh_provided(app: &AppHandle, patterns: Vec<String>) {
  let app = app.clone();
  tauri::async_runtime::spawn_blocking(move || {
    // Files' originals at those addresses too.
    super::quick_diff::refresh(&app, |uri| matches(&patterns, uri));
    let state = app.state::<EditorState>();
    let open: Vec<(DocumentId, String)> = state
      .lock()
      .documents()
      .filter_map(|document| {
        let uri = document.provided_uri()?;
        matches(&patterns, uri).then(|| (document.id(), uri.to_owned()))
      })
      .collect();
    let extensions = app.state::<ExtensionsState>();
    let Some(host) = extensions.host() else {
      return;
    };
    for (id, uri) in open {
      let text = match host.provide_text(&uri) {
        Ok(text) => text.unwrap_or_default(),
        Err(message) => {
          tracing::warn!(%uri, %message, "refreshing a provided document failed");
          continue;
        }
      };
      let summary = {
        let mut editor = state.lock();
        let Ok(document) = editor.document_mut(id) else {
          continue;
        };
        if document.provided_uri() != Some(uri.as_str())
          || !document.set_provided_text(&text).unwrap_or(false)
        {
          continue;
        }
        app.state::<ExtensionsState>().sync(document);
        DocumentSummary::of(document)
      };
      events::emit(&app, AppEvent::DocumentReloaded { document: summary });
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn patterns_match_addresses_and_prefixes() {
    let patterns = ["git:HEAD:a.rs".to_owned(), "git::*".to_owned()];
    assert!(matches(&patterns, "git:HEAD:a.rs"));
    assert!(matches(&patterns, "git::src/b.rs"));
    assert!(!matches(&patterns, "git:HEAD:b.rs"));
  }
}
