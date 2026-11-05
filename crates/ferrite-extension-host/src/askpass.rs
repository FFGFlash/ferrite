//! Signing in for extensions' programs: Git asking for a username and
//! password, or SSH for a key's passphrase, answered in Ferrite.
//!
//! A program the manifest declares with `askpass = true` runs with
//! `GIT_ASKPASS` and `SSH_ASKPASS` naming the editor's executable (from
//! [`crate::Embedder::askpass_program`]). When the program needs an answer it
//! runs that executable with the question; seeing `FERRITE_ASKPASS_*` in its
//! environment, the executable acts as a client ([`client_main`]), sends the
//! question to the server here over a local connection, and prints the answer
//! the user gives in a prompt. A token for each extension keeps other
//! programs from asking, and names the extension in the prompt.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::host::Shared;
use crate::prompts::{PromptKind, PromptReply};

const ADDR_VAR: &str = "FERRITE_ASKPASS_ADDR";
const TOKEN_VAR: &str = "FERRITE_ASKPASS_TOKEN";
/// Longest question accepted.
const MAX_REQUEST_BYTES: u64 = 8 * 1024;
/// How long the user has to answer before the program is told no.
const ANSWER_WAIT: Duration = Duration::from_secs(5 * 60);

#[derive(Serialize, Deserialize)]
struct Request {
  token: String,
  prompt: String,
}

#[derive(Serialize, Deserialize)]
struct Response {
  answer: Option<String>,
}

/// The server programs' questions come to.
pub struct Askpass {
  addr: SocketAddr,
  /// Extensions by token.
  tokens: Mutex<HashMap<String, String>>,
}

impl Askpass {
  /// Starts listening on a local port for `shared`'s extensions' programs.
  pub fn start(shared: Weak<Shared>) -> std::io::Result<Arc<Self>> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let server = Arc::new(Self {
      addr: listener.local_addr()?,
      tokens: Mutex::new(HashMap::new()),
    });
    let weak = Arc::downgrade(&server);
    std::thread::Builder::new()
      .name("askpass".into())
      .spawn(move || {
        for stream in listener.incoming().flatten() {
          let (Some(server), Some(shared)) = (weak.upgrade(), shared.upgrade()) else {
            return;
          };
          let _ = std::thread::Builder::new()
            .name("askpass-question".into())
            .spawn(move || {
              if let Err(error) = server.answer(stream, &shared) {
                tracing::debug!(%error, "a sign-in question went unanswered");
              }
            });
        }
      })?;
    Ok(server)
  }

  /// The environment that lets `extension`'s programs ask the user,
  /// through `program` (the editor's executable).
  pub fn env(&self, extension: &str, program: &Path) -> Vec<(String, String)> {
    let token = {
      let mut tokens = self.tokens.lock().unwrap_or_else(PoisonError::into_inner);
      match tokens.iter().find(|(_, owner)| *owner == extension) {
        Some((token, _)) => token.clone(),
        None => {
          let token = new_token();
          tokens.insert(token.clone(), extension.to_owned());
          token
        }
      }
    };
    let program = program.display().to_string();
    vec![
      ("GIT_ASKPASS".into(), program.clone()),
      ("SSH_ASKPASS".into(), program),
      // Even when there's a terminal, as ssh otherwise prefers one.
      ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
      (ADDR_VAR.into(), self.addr.to_string()),
      (TOKEN_VAR.into(), token),
    ]
  }

  /// Answers one question: asks the user, and replies.
  fn answer(&self, stream: TcpStream, shared: &Shared) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut line = String::new();
    BufReader::new((&stream).take(MAX_REQUEST_BYTES)).read_line(&mut line)?;
    let request: Request = serde_json::from_str(&line)?;
    let extension = self
      .tokens
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .get(&request.token)
      .cloned();
    let answer = match extension {
      Some(extension) => ask(shared, &extension, &request.prompt),
      None => None,
    };
    let mut reply = serde_json::to_string(&Response { answer })?;
    reply.push('\n');
    (&stream).write_all(reply.as_bytes())
  }
}

/// Asks the user `prompt` for `extension`'s program: a password, say.
fn ask(shared: &Shared, extension: &str, prompt: &str) -> Option<String> {
  let question = prompt.trim().trim_end_matches(':').trim();
  let lower = question.to_lowercase();
  let secret = ["password", "passphrase", "pin", "token"]
    .iter()
    .any(|word| lower.contains(word));
  let (id, replies) = shared.ask_user(
    extension,
    PromptKind::Input {
      title: if question.is_empty() {
        "Sign in".to_owned()
      } else {
        question.chars().take(200).collect()
      },
      value: String::new(),
      placeholder: None,
      password: secret,
    },
  );
  match replies.recv_timeout(ANSWER_WAIT) {
    Ok(PromptReply::Text { text }) => Some(text),
    Ok(_) | Err(RecvTimeoutError::Disconnected) => None,
    Err(RecvTimeoutError::Timeout) => {
      shared.withdraw_prompt(id);
      None
    }
  }
}

fn new_token() -> String {
  let mut bytes = [0; 16];
  if getrandom::fill(&mut bytes).is_err() {
    // No randomness: no token anyone could guess either, as none matches.
    return String::new();
  }
  bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Sends `prompt` to the server at `addr` and waits for the user's answer.
pub fn ask_server(addr: &str, token: &str, prompt: &str) -> Option<String> {
  let mut stream = TcpStream::connect(addr).ok()?;
  let mut request = serde_json::to_string(&Request {
    token: token.to_owned(),
    prompt: prompt.to_owned(),
  })
  .ok()?;
  request.push('\n');
  stream.write_all(request.as_bytes()).ok()?;
  let mut line = String::new();
  BufReader::new(stream).read_line(&mut line).ok()?;
  serde_json::from_str::<Response>(&line).ok()?.answer
}

/// When the editor's executable runs as a program's askpass helper: asks the
/// user and prints the answer. Returns the exit code then (0 with an answer,
/// 1 without), or `None` when it's running as the editor. Call it first
/// thing in `main`.
pub fn client_main() -> Option<i32> {
  let addr = std::env::var(ADDR_VAR).ok()?;
  let token = std::env::var(TOKEN_VAR).ok()?;
  let prompt = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
  Some(match ask_server(&addr, &token, &prompt) {
    Some(answer) => {
      println!("{answer}");
      0
    }
    None => 1,
  })
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeMap;
  use std::path::PathBuf;
  use std::sync::mpsc;

  use super::*;
  use crate::{Embedder, ExtensionHost, HostOptions, Limits, Notification, PromptReply};

  /// An embedder that passes notifications on.
  struct Relay {
    editor: Mutex<ferrite_core::Editor>,
    notifications: Mutex<mpsc::Sender<Notification>>,
  }

  impl Embedder for Relay {
    fn with_editor(&self, f: &mut dyn FnMut(&mut ferrite_core::Editor)) {
      f(&mut self.editor.lock().unwrap_or_else(PoisonError::into_inner));
    }

    fn language_of(&self, _: &Path) -> Option<String> {
      None
    }

    fn notify(&self, notification: Notification) {
      let _ = self
        .notifications
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .send(notification);
    }

    fn askpass_program(&self) -> Option<PathBuf> {
      Some(PathBuf::from("/bin/ferrite"))
    }
  }

  fn host() -> (ExtensionHost, mpsc::Receiver<Notification>) {
    let (sender, receiver) = mpsc::channel();
    let embedder = Arc::new(Relay {
      editor: Mutex::default(),
      notifications: Mutex::new(sender),
    });
    let host = ExtensionHost::new(
      embedder,
      HostOptions {
        editor_version: semver::Version::new(0, 1, 0),
        extensions_dir: None,
        development: Vec::new(),
        permissions_file: None,
        settings: BTreeMap::new(),
        programs: BTreeMap::new(),
        limits: Limits::default(),
        storage_dir: None,
      },
    );
    (host, receiver)
  }

  fn var<'a>(env: &'a [(String, String)], name: &str) -> &'a str {
    &env.iter().find(|(key, _)| key == name).expect("set").1
  }

  #[test]
  fn programs_questions_are_asked_and_answered() {
    let (host, notifications) = host();
    let env = host.shared().askpass_env("ferrite.git");
    assert_eq!(
      var(&env, "GIT_ASKPASS"),
      PathBuf::from("/bin/ferrite").display().to_string()
    );
    assert_eq!(var(&env, "SSH_ASKPASS_REQUIRE"), "force");
    let addr = var(&env, ADDR_VAR).to_owned();
    let token = var(&env, TOKEN_VAR).to_owned();
    assert_eq!(
      host.shared().askpass_env("ferrite.git"),
      env,
      "one token per extension"
    );

    let asking = {
      let (addr, token) = (addr.clone(), token.clone());
      std::thread::spawn(move || {
        ask_server(&addr, &token, "Password for 'https://a@example.com': ")
      })
    };
    let prompt = loop {
      if let Notification::Prompt(prompt) = notifications
        .recv_timeout(Duration::from_secs(10))
        .expect("a prompt")
      {
        break prompt;
      }
    };
    assert_eq!(
      prompt.kind,
      PromptKind::Input {
        title: "Password for 'https://a@example.com'".into(),
        value: String::new(),
        placeholder: None,
        password: true,
      }
    );
    host.prompt_answered(
      prompt.id,
      PromptReply::Text {
        text: "hunter2".into(),
      },
    );
    assert_eq!(asking.join().expect("asks").as_deref(), Some("hunter2"));

    // Cancelling answers nothing; a wrong token isn't asked at all.
    let asking = {
      let (addr, token) = (addr.clone(), token.clone());
      std::thread::spawn(move || ask_server(&addr, &token, "Username for 'https://example.com': "))
    };
    let prompt = loop {
      if let Notification::Prompt(prompt) = notifications
        .recv_timeout(Duration::from_secs(10))
        .expect("a prompt")
      {
        break prompt;
      }
    };
    assert!(matches!(
      prompt.kind,
      PromptKind::Input {
        password: false,
        ..
      }
    ));
    host.prompt_answered(prompt.id, PromptReply::Cancelled);
    assert_eq!(asking.join().expect("asks"), None);
    assert_eq!(ask_server(&addr, "wrong", "Password: "), None);
  }
}
