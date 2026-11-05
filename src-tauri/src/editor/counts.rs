//! Background find-match counting for large documents.
//!
//! Counting every match means scanning the whole document, which is too slow
//! to do on each keystroke for large files. Views of large documents show the
//! visible matches right away and schedule a count on the shared worker pool
//! over a snapshot; when it finishes, the frontend is told to refresh.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use flareite_core::{Document, DocumentId, MAX_MATCHES};
use flareite_tasks::Priority;
use flareite_text::{SearchQuery, Version};
use tauri::{AppHandle, Manager};

use super::protocol::MatchCount;
use crate::events::{self, AppEvent};
use crate::perf::Perf;

#[derive(Default)]
struct Entry {
  done: Option<Arc<MatchCount>>,
  pending: Option<(Version, SearchQuery)>,
}

#[derive(Default)]
pub struct MatchCounts {
  entries: Mutex<HashMap<DocumentId, Entry>>,
}

impl MatchCounts {
  fn lock(&self) -> MutexGuard<'_, HashMap<DocumentId, Entry>> {
    self.entries.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// The latest finished count for `id`, possibly for an older version or
  /// another query; callers check.
  pub fn get(&self, id: DocumentId) -> Option<Arc<MatchCount>> {
    self.lock().get(&id).and_then(|entry| entry.done.clone())
  }

  /// Schedules a count of `query` in `document` unless one for the same
  /// version and query is already pending. A newer request for the same
  /// document cancels an older one.
  pub fn schedule(&self, app: &AppHandle, document: &Document, query: &SearchQuery) {
    let id = document.id();
    let version = document.version();
    {
      let mut entries = self.lock();
      let entry = entries.entry(id).or_default();
      if entry.pending.as_ref() == Some(&(version, query.clone())) {
        return;
      }
      entry.pending = Some((version, query.clone()));
    }
    let snapshot = document.buffer().snapshot();
    let query = query.clone();
    let app = app.clone();
    app.state::<Perf>().scheduler.spawn(
      "count-matches",
      Priority::High,
      Some(format!("count:{id}")),
      {
        let app = app.clone();
        move |token| {
          let started = Instant::now();
          let Ok(searcher) = query.compile() else {
            return;
          };
          let mut matches = searcher.find_in_snapshot(&snapshot, MAX_MATCHES + 1);
          if token.is_cancelled() {
            return;
          }
          let truncated = matches.len() > MAX_MATCHES;
          matches.truncate(MAX_MATCHES);
          app
            .state::<Perf>()
            .record("search.count", started.elapsed());

          let counts = &app.state::<super::EditorState>().counts;
          {
            let mut entries = counts.lock();
            let entry = entries.entry(id).or_default();
            if entry.pending.as_ref() == Some(&(version, query.clone())) {
              entry.pending = None;
            }
            entry.done = Some(Arc::new(MatchCount {
              version,
              query,
              matches,
              truncated,
            }));
          }
          events::emit(
            &app,
            AppEvent::SearchCounted {
              document_id: id.get(),
            },
          );
        }
      },
    );
  }

  /// Drops results for a closed document.
  pub fn forget(&self, id: DocumentId) {
    self.lock().remove(&id);
  }
}
