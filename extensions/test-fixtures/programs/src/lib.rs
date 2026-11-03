//! A test fixture for the API 1.6 interfaces: it runs the Rust compiler,
//! watches paths, marks files in the explorer, and reports the open folder.
//! The host's integration tests run its commands; outcomes are reported
//! with `show_message`.

use ferrite_extension_api::explorer::{self, FileMark};
use ferrite_extension_api::processes::{self, ProcessOptions, ProcessOutput};
use ferrite_extension_api::types::{
  Activation, CommandContext, DocumentChange, DocumentId, DocumentInfo, MessageLevel,
};
use ferrite_extension_api::{Guest, export_extension, host, workbench, workspace};

fn report(message: impl AsRef<str>) {
  host::show_message(MessageLevel::Info, message.as_ref());
}

fn options() -> ProcessOptions {
  ProcessOptions {
    cwd: None,
    env: Vec::new(),
    stdin: None,
    timeout_ms: None,
  }
}

/// The exit code and the first word of standard output.
fn summary(output: &ProcessOutput) -> String {
  let stdout = String::from_utf8_lossy(&output.stdout);
  let word = stdout.split_whitespace().next().unwrap_or_default();
  format!("{:?} {word}", output.exit_code)
}

fn mark(path: String, color: &str) -> FileMark {
  FileMark {
    path,
    badge: Some("M".into()),
    color: Some(color.into()),
    tooltip: Some("Modified".into()),
    faded: false,
    inherit: false,
    propagate: true,
    priority: 3,
  }
}

struct Programs;

impl Guest for Programs {
  fn activate(_: Activation) -> Result<(), String> {
    Ok(())
  }

  fn deactivate() {}

  fn run_command(command: String, _: CommandContext) -> Result<(), String> {
    let argument = workbench::command_argument().unwrap_or_default();
    match command.trim_start_matches("test.programs.") {
      "run" => {
        let result = processes::run("rustc", &["--version".into()], &options());
        report(match result {
          Ok(output) => format!("run: {}", summary(&output)),
          Err(error) => format!("run: {error:?}"),
        });
      }
      "unknown" => {
        let result = processes::run("sh", &[], &options());
        report(format!("unknown: {:?}", result.err()));
      }
      "spawn" => {
        let result = processes::spawn(
          "rustc",
          &["--version".into()],
          &options(),
          "test.programs.finished",
        );
        report(format!("spawn: {}", result.is_ok()));
      }
      "finished" => {
        let finished = processes::finished().ok_or("not run for a program")?;
        report(match finished.outcome {
          Ok(output) => format!("finished: {}", summary(&output)),
          Err(error) => format!("finished: {error:?}"),
        });
      }
      "watch" => {
        let result = workspace::watch(&argument, "test.programs.changed");
        report(format!(
          "watch: {}",
          result.map(|_| "ok").unwrap_or("failed")
        ));
      }
      "changed" => {
        let changes = workspace::changes().ok_or("not run for changes")?;
        report(format!("changed: {}", changes.paths.join(";")));
      }
      "mark" => {
        let result = explorer::set_file_marks(&[mark(argument, "modified")]);
        report(format!("mark: {result:?}"));
      }
      "badMark" => {
        let result = explorer::set_file_marks(&[mark(argument, "nope")]);
        report(format!("badMark: {}", result.is_err()));
      }
      "folder" => {
        let folder = workspace::folder();
        report(format!("folder: {:?}", folder.map(|folder| folder.trusted)));
      }
      other => return Err(format!("unknown command {other}")),
    }
    Ok(())
  }

  fn document_opened(_: DocumentInfo) {}
  fn document_changed(_: DocumentChange) {}
  fn document_closed(_: DocumentId) {}
}

export_extension!(Programs);
