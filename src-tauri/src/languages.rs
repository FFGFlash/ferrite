//! The languages the editor knows and their language servers, all from
//! extensions: Ferrite has none built in, so without language extensions
//! every file is plain text.
//!
//! Rebuilt in the background whenever extensions change. A language whose
//! definition and files are unchanged keeps its registration, so its grammar
//! isn't compiled again; a changed one (say, an edited query after
//! restarting a development extension) gets a new registration, and open
//! documents of it are parsed again.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::SystemTime;

use ferrite_extension_host::contributions::{LanguageContribution, LanguageServerContribution};
use ferrite_lsp::{ServerCommand, ServerConfig};
use ferrite_syntax::{Language, LanguageDefinition, Registry};
use ferrite_tasks::Priority;
use tauri::{AppHandle, Manager};

use crate::editor::EditorState;
use crate::events::{self, AppEvent};
use crate::extensions::ExtensionsState;
use crate::lsp::LspState;
use crate::perf::Perf;

#[derive(Default)]
pub struct LanguagesState {
  current: RwLock<Current>,
  /// Held while rebuilding, so rebuilds don't overtake each other.
  rebuilding: Mutex<()>,
}

#[derive(Default)]
struct Current {
  registry: Arc<Registry>,
  /// What `registry` was built from, with when each file was modified.
  built_from: Vec<(LanguageDefinition, Vec<Option<SystemTime>>)>,
  servers: Vec<ServerConfig>,
}

impl LanguagesState {
  pub fn registry(&self) -> Arc<Registry> {
    self.read(|current| current.registry.clone())
  }

  /// The language of a file, if it has one.
  pub fn for_path(&self, path: &Path) -> Option<Language> {
    self.read(|current| current.registry.for_path(path).cloned())
  }

  /// Language servers declared by extensions the user allowed to run them.
  pub fn servers(&self) -> Vec<ServerConfig> {
    self.read(|current| current.servers.clone())
  }

  fn read<T>(&self, f: impl FnOnce(&Current) -> T) -> T {
    f(&self.current.read().unwrap_or_else(PoisonError::into_inner))
  }
}

/// Rebuilds the languages from the extensions in the background.
pub fn refresh(app: &AppHandle) {
  let task_app = app.clone();
  app.state::<Perf>().scheduler.spawn(
    "languages",
    Priority::High,
    Some("languages".into()),
    move |_| rebuild(&task_app),
  );
}

fn rebuild(app: &AppHandle) {
  let extensions = app.state::<ExtensionsState>();
  let Some(host) = extensions.host() else {
    return;
  };
  let state = app.state::<LanguagesState>();
  let _rebuilding = state
    .rebuilding
    .lock()
    .unwrap_or_else(PoisonError::into_inner);
  let built_from: Vec<_> = host
    .languages()
    .into_iter()
    .map(|language| {
      let definition = definition(language);
      let stamps = stamps(&definition);
      (definition, stamps)
    })
    .collect();
  let servers: Vec<ServerConfig> = host
    .language_servers()
    .into_iter()
    .map(server_config)
    .collect();
  let (languages_changed, servers_changed) =
    state.read(|current| (current.built_from != built_from, current.servers != servers));

  if languages_changed {
    let (registry, problems) = Registry::new(built_from.iter().map(|(d, _)| d.clone()));
    for problem in problems {
      tracing::warn!(%problem, "an extension's language was left out");
    }
    tracing::info!(
      languages = ?built_from.iter().map(|(d, _)| &d.id).collect::<Vec<_>>(),
      "extension languages changed"
    );
    let mut current = state
      .current
      .write()
      .unwrap_or_else(PoisonError::into_inner);
    current.registry = Arc::new(registry);
    current.built_from = built_from;
  }
  if servers_changed {
    state
      .current
      .write()
      .unwrap_or_else(PoisonError::into_inner)
      .servers = servers;
    // Starts over with the new servers, stopping any no longer allowed.
    app.state::<LspState>().restart_all(app);
  }
  if languages_changed {
    host.languages_changed();
    // Documents of a new language may now have a server.
    if !servers_changed {
      let editor_state = app.state::<EditorState>();
      let editor = editor_state.lock();
      for document in editor.documents() {
        app.state::<LspState>().sync(app, document);
      }
    }
    events::emit(app, AppEvent::LanguagesChanged);
  }
}

fn definition(language: LanguageContribution) -> LanguageDefinition {
  LanguageDefinition {
    id: language.id,
    name: language.name,
    file_extensions: language.file_extensions,
    file_names: language.file_names,
    grammar: language.grammar,
    grammar_name: language.grammar_name,
    highlights: language.highlights,
    injections: language.injections,
  }
}

/// When each of a language's files was last modified.
fn stamps(definition: &LanguageDefinition) -> Vec<Option<SystemTime>> {
  [&definition.grammar, &definition.highlights]
    .into_iter()
    .chain(&definition.injections)
    .map(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok())
    .collect()
}

fn server_config(server: LanguageServerContribution) -> ServerConfig {
  ServerConfig {
    name: server.name,
    command: ServerCommand {
      program: server.command,
      args: server.args,
    },
    languages: server.languages,
    root_markers: server.root_markers,
  }
}

/// The languages of the example extensions in `extensions/` (e.g. `rust`),
/// loaded from their manifests, for tests.
#[cfg(test)]
pub fn examples(folders: &[&str]) -> Registry {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../extensions");
  let definitions = folders.iter().flat_map(|folder| {
    let dir = root.join(folder);
    let manifest =
      ferrite_extension_host::manifest::load(&dir, &semver::Version::new(0, 1, 0), true)
        .unwrap_or_else(|error| panic!("{folder}: {error}"));
    manifest
      .contributions
      .languages
      .into_iter()
      .map(move |language| LanguageContribution {
        grammar: dir.join(&language.grammar),
        highlights: dir.join(&language.highlights),
        injections: language.injections.as_ref().map(|path| dir.join(path)),
        ..language
      })
      .map(definition)
  });
  let (registry, problems) = Registry::new(definitions);
  assert!(problems.is_empty(), "{problems:?}");
  registry
}
