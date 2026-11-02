//! A scripted language server for `ferrite-lsp`'s tests.
//!
//! It keeps each open document's text by applying the changes it receives
//! (counting columns its own way, independently of the client), reports a
//! diagnostic for every occurrence of the word `error`, and answers hover,
//! definition, references and completion requests from the text. `fake/text` returns a
//! document's text so tests can check synchronization; `fake/sleep` delays
//! its answer. Flags: `--utf16` negotiates UTF-16 positions instead of
//! UTF-8; `--crash-on-hover` exits when asked for a hover.

use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::time::Duration;

use ferrite_lsp::framing::{read_message, write_message};
use serde_json::{Value, json};

struct Server {
  utf16: bool,
  crash_on_hover: bool,
  documents: HashMap<String, String>,
  out: std::io::Stdout,
}

impl Server {
  fn send(&mut self, message: Value) {
    let body = serde_json::to_vec(&message).expect("serializes");
    write_message(&mut self.out, &body).expect("writes");
    self.out.flush().expect("flushes");
  }

  /// The byte offset of an LSP position, lines split on `\n`.
  fn offset(&self, text: &str, position: &Value) -> usize {
    let line = position["line"].as_u64().unwrap_or(0) as usize;
    let character = position["character"].as_u64().unwrap_or(0) as usize;
    let mut start = 0;
    for _ in 0..line {
      match text[start..].find('\n') {
        Some(index) => start += index + 1,
        None => return text.len(),
      }
    }
    let line_text = &text[start
      ..text[start..]
        .find('\n')
        .map_or(text.len(), |end| start + end)];
    let mut units = 0;
    for (index, ch) in line_text.char_indices() {
      if units >= character {
        return start + index;
      }
      units += if self.utf16 {
        ch.len_utf16()
      } else {
        ch.len_utf8()
      };
    }
    start + line_text.len()
  }

  fn position(&self, text: &str, offset: usize) -> Value {
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column: usize = if self.utf16 {
      before[line_start..].chars().map(char::len_utf16).sum()
    } else {
      offset - line_start
    };
    json!({ "line": line, "character": column })
  }

  fn publish(&mut self, uri: &str, version: Option<&Value>) {
    let text = self.documents.get(uri).cloned().unwrap_or_default();
    let diagnostics: Vec<Value> = text
      .match_indices("error")
      .map(|(offset, word)| {
        json!({
          "range": { "start": self.position(&text, offset), "end": self.position(&text, offset + word.len()) },
          "severity": 1,
          "source": "fake",
          "message": "found error",
        })
      })
      .collect();
    self.send(json!({
      "jsonrpc": "2.0",
      "method": "textDocument/publishDiagnostics",
      "params": { "uri": uri, "version": version, "diagnostics": diagnostics },
    }));
  }

  fn word_at(&self, params: &Value) -> (String, String) {
    let uri = params["textDocument"]["uri"]
      .as_str()
      .unwrap_or_default()
      .to_owned();
    let text = self.documents.get(&uri).cloned().unwrap_or_default();
    let offset = self.offset(&text, &params["position"]);
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    let start = text[..offset]
      .char_indices()
      .rev()
      .find(|&(_, ch)| !is_word(ch))
      .map_or(0, |(index, ch)| index + ch.len_utf8());
    let end = text[offset..]
      .char_indices()
      .find(|&(_, ch)| !is_word(ch))
      .map_or(text.len(), |(index, _)| offset + index);
    (uri, text[start..end].to_owned())
  }

  fn handle(&mut self, message: Value) {
    let method = message["method"].as_str().unwrap_or_default().to_owned();
    let params = message["params"].clone();
    let id = message.get("id").cloned();
    let result = match method.as_str() {
      "initialize" => {
        // Exercise the client's handling of server-to-client requests.
        self.send(json!({
          "jsonrpc": "2.0", "id": "config-1", "method": "workspace/configuration",
          "params": { "items": [{ "section": "fake" }] },
        }));
        json!({
          "capabilities": {
            "positionEncoding": if self.utf16 { "utf-16" } else { "utf-8" },
            "textDocumentSync": { "openClose": true, "change": 2, "save": { "includeText": false } },
            "hoverProvider": true,
            "definitionProvider": true,
            "referencesProvider": true,
            "completionProvider": { "triggerCharacters": ["."] },
          },
          "serverInfo": { "name": "fake", "version": "1" },
        })
      }
      "textDocument/didOpen" => {
        let uri = params["textDocument"]["uri"]
          .as_str()
          .unwrap_or_default()
          .to_owned();
        let text = params["textDocument"]["text"]
          .as_str()
          .unwrap_or_default()
          .to_owned();
        self.documents.insert(uri.clone(), text);
        self.publish(&uri, Some(&params["textDocument"]["version"]));
        return;
      }
      "textDocument/didChange" => {
        let uri = params["textDocument"]["uri"]
          .as_str()
          .unwrap_or_default()
          .to_owned();
        let mut text = self.documents.get(&uri).cloned().unwrap_or_default();
        for change in params["contentChanges"].as_array().into_iter().flatten() {
          let new_text = change["text"].as_str().unwrap_or_default();
          match change.get("range") {
            Some(range) => {
              let start = self.offset(&text, &range["start"]);
              let end = self.offset(&text, &range["end"]);
              text.replace_range(start..end, new_text);
            }
            None => text = new_text.to_owned(),
          }
        }
        self.documents.insert(uri.clone(), text);
        self.publish(&uri, Some(&params["textDocument"]["version"]));
        return;
      }
      "textDocument/didClose" => {
        let uri = params["textDocument"]["uri"]
          .as_str()
          .unwrap_or_default()
          .to_owned();
        self.documents.remove(&uri);
        return;
      }
      "textDocument/hover" => {
        if self.crash_on_hover {
          std::process::exit(3);
        }
        let (_, word) = self.word_at(&params);
        json!({ "contents": { "kind": "markdown", "value": format!("word `{word}`") } })
      }
      "textDocument/definition" => {
        let (uri, word) = self.word_at(&params);
        let text = self.documents.get(&uri).cloned().unwrap_or_default();
        match text.find(&word) {
          Some(offset) if !word.is_empty() => json!({
            "uri": uri,
            "range": { "start": self.position(&text, offset), "end": self.position(&text, offset + word.len()) },
          }),
          _ => Value::Null,
        }
      }
      "textDocument/references" => {
        let (uri, word) = self.word_at(&params);
        let text = self.documents.get(&uri).cloned().unwrap_or_default();
        let locations: Vec<Value> = if word.is_empty() {
          Vec::new()
        } else {
          text
            .match_indices(&word)
            .map(|(offset, _)| {
              json!({
                "uri": uri,
                "range": { "start": self.position(&text, offset), "end": self.position(&text, offset + word.len()) },
              })
            })
            .collect()
        };
        json!(locations)
      }
      "textDocument/completion" => json!({
        "isIncomplete": false,
        "items": [{ "label": "alpha", "kind": 3 }, { "label": "beta", "kind": 6, "detail": "a variable" }],
      }),
      "fake/text" => {
        let uri = params["uri"].as_str().unwrap_or_default();
        json!(self.documents.get(uri))
      }
      "fake/sleep" => {
        std::thread::sleep(Duration::from_millis(params["ms"].as_u64().unwrap_or(0)));
        Value::Null
      }
      "shutdown" => Value::Null,
      "exit" => std::process::exit(0),
      _ => {
        // Responses to our own requests have no method and need no answer.
        if let Some(id) = id
          && message.get("method").is_some()
        {
          self.send(json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": format!("{method} not found") },
          }));
        }
        return;
      }
    };
    if let Some(id) = id {
      self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }
  }
}

fn main() {
  let args: Vec<String> = std::env::args().collect();
  let mut server = Server {
    utf16: args.iter().any(|arg| arg == "--utf16"),
    crash_on_hover: args.iter().any(|arg| arg == "--crash-on-hover"),
    documents: HashMap::new(),
    out: std::io::stdout(),
  };
  let mut input = BufReader::new(std::io::stdin());
  while let Ok(Some(body)) = read_message(&mut input) {
    if let Ok(message) = serde_json::from_slice::<Value>(&body) {
      server.handle(message);
    }
  }
}
