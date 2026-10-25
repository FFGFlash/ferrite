//! A test fixture: an extension that breaks rules and exceeds limits when
//! asked to. The host's integration tests run its commands and check that
//! the host stays healthy. Outcomes are reported with `show_message`.

use ferrite_extension_api::types::{
  Activation, CommandContext, ConfigValue, Diagnostic, DocumentChange, DocumentId, DocumentInfo,
  LogLevel, MessageLevel, Position, Range, Severity, TextEdit,
};
use ferrite_extension_api::{Guest, export_extension, host};

fn text_setting(key: &str) -> String {
  match host::configuration(key) {
    Some(ConfigValue::Text(text)) => text,
    other => format!("{other:?}"),
  }
}

fn report(message: impl AsRef<str>) {
  host::show_message(MessageLevel::Info, message.as_ref());
}

fn at(line: u32, column: u32) -> Position {
  Position { line, column }
}

fn edit(start: Position, end: Position, text: &str) -> TextEdit {
  TextEdit {
    range: Range { start, end },
    text: text.into(),
  }
}

#[allow(unconditional_recursion)]
fn recurse(depth: u64) -> u64 {
  recurse(depth + 1).wrapping_add(std::hint::black_box(depth))
}

fn spin() -> ! {
  loop {
    std::hint::black_box(());
  }
}

struct Misbehaving;

impl Guest for Misbehaving {
  fn activate(context: Activation) -> Result<(), String> {
    match text_setting("on_activate").as_str() {
      "error" => Err("activation refused".into()),
      "loop" => spin(),
      _ => {
        report(format!("activated with {:?}", context.granted));
        Ok(())
      }
    }
  }

  fn deactivate() {}

  fn run_command(command: String, context: CommandContext) -> Result<(), String> {
    let document = context.document.unwrap_or(0);
    let version = host::document(document).map_or(0, |info| info.version);
    match command.trim_start_matches("test.misbehaving.") {
      "loop" => spin(),
      "alloc" => {
        let mut blocks = Vec::new();
        loop {
          blocks.push(vec![1u8; 1 << 20]);
          std::hint::black_box(&blocks);
        }
      }
      "panic" => panic!("deliberate panic"),
      "recurse" => {
        std::hint::black_box(recurse(0));
      }
      "escape" => {
        let file = std::fs::read_to_string("Cargo.toml").map(|text| text.len());
        let env = std::env::var("PATH");
        let args = std::env::args().count();
        report(format!("file: {file:?}; env: {env:?}; args: {args}"));
      }
      "edit" => {
        let result = host::apply_edits(document, version, &[edit(at(0, 0), at(0, 0), "X")]);
        report(format!("edit: {result:?}"));
      }
      "editStale" => {
        let edits = [edit(at(0, 0), at(0, 0), "X")];
        let result = host::apply_edits(document, version.saturating_sub(1), &edits);
        report(format!("edit: {result:?}"));
      }
      "editBadRange" => {
        let result = host::apply_edits(document, version, &[edit(at(0, 0), at(0, 999), "X")]);
        report(format!("edit: {result:?}"));
      }
      "editOverlapping" => {
        let edits = [edit(at(0, 0), at(0, 3), "A"), edit(at(0, 2), at(0, 4), "B")];
        let result = host::apply_edits(document, version, &edits);
        report(format!("edit: {result:?}"));
      }
      "read" => {
        let result = host::read_lines(document, 0, 1).map(|slice| slice.lines);
        report(format!("read: {result:?}"));
      }
      "flood" => {
        let long = "x".repeat(100_000);
        for _ in 0..10_000 {
          host::log(LogLevel::Info, &long);
        }
        let diagnostic = Diagnostic {
          range: Range {
            start: at(0, 0),
            end: at(0, 0),
          },
          severity: Severity::Error,
          message: "too many".into(),
          code: None,
        };
        let many = vec![diagnostic; 6_000];
        let result = host::publish_diagnostics(document, version, &many);
        report(format!("diagnostics: {result:?}"));
      }
      "diagnose" => {
        let diagnostic = Diagnostic {
          range: Range {
            start: at(0, 0),
            end: at(0, 1),
          },
          severity: Severity::Warning,
          message: "first character".into(),
          code: Some("first".into()),
        };
        let result = host::publish_diagnostics(document, version, &[diagnostic]);
        report(format!("diagnostics: {result:?}"));
      }
      "config" => {
        report(format!(
          "on_change: {}; limit: {:?}; unknown: {:?}",
          text_setting("on_change"),
          host::configuration("limit"),
          host::configuration("unknown"),
        ));
      }
      other => return Err(format!("unknown command {other}")),
    }
    Ok(())
  }

  fn document_opened(document: DocumentInfo) {
    report(format!("opened {} v{}", document.id, document.version));
  }

  fn document_changed(change: DocumentChange) {
    if text_setting("on_change") == "loop" {
      spin();
    }
    let changes = match change.changes {
      Some(changes) => changes
        .iter()
        .map(|c| {
          let (start, end) = (c.range.start, c.range.end);
          format!(
            "{}:{}-{}:{}={:?}",
            start.line, start.column, end.line, end.column, c.text
          )
        })
        .collect::<Vec<_>>()
        .join(","),
      None => "reread".into(),
    };
    report(format!(
      "changed {} v{} [{changes}]",
      change.document, change.version
    ));
  }

  fn document_closed(document: DocumentId) {
    report(format!("closed {document}"));
  }
}

export_extension!(Misbehaving);
