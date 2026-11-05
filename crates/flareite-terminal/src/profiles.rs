//! Terminal profiles: the shells installed on this computer, found where
//! their installers put them (as VS Code finds them).
//!
//! - **Windows:** PowerShell 7, Windows PowerShell, Command Prompt, Git Bash,
//!   Cygwin, MSYS2, and each WSL distribution.
//! - **Elsewhere:** the shells listed in `/etc/shells`, and `$SHELL`.
//!
//! The system is reached through [`Probe`], so detection is testable with a
//! fake one.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::shell::{Shell, find_on_path};

/// A shell to start terminals with, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
  /// Shown to the user, and how settings refer to it, such as `Git Bash`
  /// or `Ubuntu (WSL)`.
  pub name: String,
  pub shell: Shell,
  /// Added to the terminal's environment.
  pub env: Vec<(String, String)>,
}

impl Profile {
  fn new(name: &str, program: &Path, args: &[&str]) -> Self {
    Self {
      name: name.to_owned(),
      shell: Shell {
        program: program.display().to_string(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
      },
      env: Vec::new(),
    }
  }

  fn with_env(mut self, env: &[(&str, &str)]) -> Self {
    self.env = env
      .iter()
      .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
      .collect();
    self
  }
}

/// What detection needs to know about the system.
pub trait Probe {
  fn var(&self, name: &str) -> Option<String>;
  fn is_file(&self, path: &Path) -> bool;
  /// The directories in `path` (for versioned install folders).
  fn subdirectories(&self, path: &Path) -> Vec<PathBuf>;
  fn which(&self, program: &str) -> Option<PathBuf>;
  /// The output of `wsl.exe --list --quiet`, if WSL is installed.
  fn wsl_list(&self) -> Option<Vec<u8>>;
  /// The contents of `/etc/shells`.
  fn etc_shells(&self) -> Option<String>;
}

/// The real system.
pub struct System;

impl Probe for System {
  fn var(&self, name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
  }

  fn is_file(&self, path: &Path) -> bool {
    path.is_file()
  }

  fn subdirectories(&self, path: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(path)
      .map(|entries| {
        entries
          .flatten()
          .map(|entry| entry.path())
          .filter(|path| path.is_dir())
          .collect()
      })
      .unwrap_or_default();
    dirs.sort();
    dirs
  }

  fn which(&self, program: &str) -> Option<PathBuf> {
    find_on_path(program)
  }

  fn wsl_list(&self) -> Option<Vec<u8>> {
    let wsl = Path::new(&self.var("SystemRoot")?).join(r"System32\wsl.exe");
    if !wsl.is_file() {
      return None;
    }
    let mut command = Command::new(wsl);
    command.args(["--list", "--quiet"]).env("WSL_UTF8", "1");
    hide_window(&mut command);
    let output = command.output().ok()?;
    output.status.success().then_some(output.stdout)
  }

  fn etc_shells(&self) -> Option<String> {
    std::fs::read_to_string("/etc/shells").ok()
  }
}

/// Keeps a console program started from a GUI app from flashing a window.
#[cfg(windows)]
fn hide_window(command: &mut Command) {
  use std::os::windows::process::CommandExt;
  const CREATE_NO_WINDOW: u32 = 0x0800_0000;
  command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}

/// The profiles installed on this computer, in the order they're offered.
pub fn detect() -> Vec<Profile> {
  if cfg!(windows) {
    windows_profiles(&System)
  } else {
    unix_profiles(&System)
  }
}

/// The profile used when the user hasn't chosen one: PowerShell 7, else
/// Windows PowerShell, on Windows; `$SHELL` elsewhere.
pub fn default_profile<'a>(profiles: &'a [Profile], probe: &dyn Probe) -> Option<&'a Profile> {
  let by_name = |name: &str| profiles.iter().find(|profile| profile.name == name);
  if cfg!(windows) {
    by_name("PowerShell")
      .or_else(|| by_name("Windows PowerShell"))
      .or_else(|| profiles.first())
  } else {
    probe
      .var("SHELL")
      .and_then(|shell| by_name(&base_name(Path::new(&shell))))
      .or_else(|| profiles.first())
  }
}

/// Windows profiles, in VS Code's order.
pub fn windows_profiles(probe: &dyn Probe) -> Vec<Profile> {
  let mut profiles = Vec::new();
  let var_path = |name: &str| probe.var(name).map(PathBuf::from);
  let system32 = var_path("SystemRoot").map(|root| root.join("System32"));
  let program_files: Vec<PathBuf> = ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
    .into_iter()
    .filter_map(var_path)
    .collect();
  let first_file =
    |candidates: Vec<PathBuf>| candidates.into_iter().find(|path| probe.is_file(path));

  // PowerShell 7 and later: on the PATH, or the newest version installed.
  let pwsh = probe.which("pwsh.exe").or_else(|| {
    let mut versions: Vec<PathBuf> = program_files
      .iter()
      .flat_map(|dir| probe.subdirectories(&dir.join("PowerShell")))
      .map(|dir| dir.join("pwsh.exe"))
      .filter(|path| probe.is_file(path))
      .collect();
    versions.sort();
    versions.pop()
  });
  if let Some(pwsh) = pwsh {
    profiles.push(Profile::new("PowerShell", &pwsh, &[]));
  }
  if let Some(system32) = &system32 {
    let powershell = system32.join(r"WindowsPowerShell\v1.0\powershell.exe");
    if probe.is_file(&powershell) {
      profiles.push(Profile::new("Windows PowerShell", &powershell, &[]));
    }
    let cmd = system32.join("cmd.exe");
    if probe.is_file(&cmd) {
      profiles.push(Profile::new("Command Prompt", &cmd, &[]));
    }
  }

  // Git Bash: where the installer puts it, or beside `git` on the PATH
  // (`<Git>\cmd\git.exe` has `<Git>\bin\bash.exe`).
  let mut git_dirs: Vec<PathBuf> = program_files.iter().map(|dir| dir.join("Git")).collect();
  if let Some(local) = var_path("LOCALAPPDATA") {
    git_dirs.push(local.join(r"Programs\Git"));
  }
  if let Some(git) = probe.which("git.exe")
    && let Some(root) = git.parent().and_then(Path::parent)
  {
    git_dirs.push(root.to_path_buf());
  }
  if let Some(bash) = first_file(
    git_dirs
      .iter()
      .map(|dir| dir.join(r"bin\bash.exe"))
      .collect(),
  ) {
    profiles.push(Profile::new("Git Bash", &bash, &["--login", "-i"]));
  }

  // Cygwin and MSYS2 log in to their home folder unless told to stay.
  let stay = ("CHERE_INVOKING", "1");
  let system_drive = probe.var("SystemDrive").unwrap_or_else(|| "C:".to_owned());
  let root = PathBuf::from(format!(r"{system_drive}\"));
  if let Some(bash) = first_file(vec![
    root.join(r"cygwin64\bin\bash.exe"),
    root.join(r"cygwin\bin\bash.exe"),
  ]) {
    profiles.push(Profile::new("Cygwin", &bash, &["--login", "-i"]).with_env(&[stay]));
  }
  if let Some(bash) = first_file(vec![root.join(r"msys64\usr\bin\bash.exe")]) {
    profiles.push(
      Profile::new("MSYS2", &bash, &["--login", "-i"]).with_env(&[stay, ("MSYSTEM", "UCRT64")]),
    );
  }

  // Each WSL distribution, started in the folder the terminal opens in.
  if let (Some(system32), Some(list)) = (&system32, probe.wsl_list()) {
    let wsl = system32.join("wsl.exe");
    for distribution in parse_wsl_list(&list) {
      profiles.push(Profile::new(
        &format!("{distribution} (WSL)"),
        &wsl,
        &["--distribution", &distribution],
      ));
    }
  }
  profiles
}

/// The distributions in `wsl.exe --list --quiet` output, which is UTF-16
/// unless `WSL_UTF8` is understood. Docker Desktop's are its own, not shells.
pub fn parse_wsl_list(output: &[u8]) -> Vec<String> {
  // Distribution names are ASCII, so UTF-16 shows as a zero after each byte.
  let output = output.strip_prefix(&[0xff, 0xfe]).unwrap_or(output);
  let utf16 = output.len() >= 2
    && output.len().is_multiple_of(2)
    && output.iter().skip(1).step_by(2).all(|byte| *byte == 0);
  let text = if utf16 {
    let units: Vec<u16> = output
      .as_chunks::<2>()
      .0
      .iter()
      .map(|pair| u16::from_le_bytes(*pair))
      .collect();
    String::from_utf16_lossy(&units)
  } else {
    String::from_utf8_lossy(output).into_owned()
  };
  text
    .lines()
    .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0'))
    .filter(|name| !name.is_empty() && !name.starts_with("docker-desktop"))
    .map(str::to_owned)
    .collect()
}

/// Programs some systems list in `/etc/shells` that aren't for a terminal,
/// or only lock one down (`rbash` is restricted bash).
const NOT_SHELLS: &[&str] = &["false", "nologin", "git-shell", "rbash", "screen", "tmux"];

/// The shells in `/etc/shells` that exist, one per name, and `$SHELL`.
pub fn unix_profiles(probe: &dyn Probe) -> Vec<Profile> {
  // macOS runs shells as login shells, as Terminal.app does, so profile
  // scripts set up the PATH for apps started from the Dock.
  let args: &[&str] = if cfg!(target_os = "macos") {
    &["-l"]
  } else {
    &[]
  };
  let listed = probe.etc_shells().unwrap_or_default();
  let mut paths: Vec<PathBuf> = probe.var("SHELL").map(PathBuf::from).into_iter().collect();
  paths.extend(
    listed
      .lines()
      .map(str::trim)
      .filter(|line| line.starts_with('/'))
      .map(PathBuf::from),
  );
  let mut seen = HashSet::new();
  let mut profiles = Vec::new();
  for path in paths {
    let name = base_name(&path);
    if NOT_SHELLS.contains(&name.as_str()) || !probe.is_file(&path) || !seen.insert(name.clone()) {
      continue;
    }
    profiles.push(Profile::new(&name, &path, args));
  }
  // `$SHELL` went first, so its path wins for its name; offer them by name.
  profiles.sort_by(|a, b| a.name.cmp(&b.name));
  profiles
}

fn base_name(path: &Path) -> String {
  path
    .file_name()
    .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use super::*;

  #[derive(Default)]
  struct Fake {
    vars: HashMap<&'static str, &'static str>,
    files: Vec<PathBuf>,
    dirs: HashMap<PathBuf, Vec<PathBuf>>,
    path: HashMap<&'static str, PathBuf>,
    wsl: Option<Vec<u8>>,
    etc_shells: Option<&'static str>,
  }

  impl Probe for Fake {
    fn var(&self, name: &str) -> Option<String> {
      self.vars.get(name).map(|value| (*value).to_owned())
    }
    fn is_file(&self, path: &Path) -> bool {
      self.files.iter().any(|file| file == path)
    }
    fn subdirectories(&self, path: &Path) -> Vec<PathBuf> {
      self.dirs.get(path).cloned().unwrap_or_default()
    }
    fn which(&self, program: &str) -> Option<PathBuf> {
      self.path.get(program).cloned()
    }
    fn wsl_list(&self) -> Option<Vec<u8>> {
      self.wsl.clone()
    }
    fn etc_shells(&self) -> Option<String> {
      self.etc_shells.map(str::to_owned)
    }
  }

  fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
  }

  fn windows() -> Fake {
    let system32 = Path::new(r"C:\Windows").join("System32");
    let program_files = PathBuf::from(r"C:\Program Files");
    Fake {
      vars: HashMap::from([
        ("SystemRoot", r"C:\Windows"),
        ("SystemDrive", "C:"),
        ("ProgramFiles", r"C:\Program Files"),
      ]),
      files: vec![
        system32.join(r"WindowsPowerShell\v1.0\powershell.exe"),
        system32.join("cmd.exe"),
        program_files.join("Git").join(r"bin\bash.exe"),
        PathBuf::from(r"C:\").join(r"msys64\usr\bin\bash.exe"),
      ],
      wsl: Some(utf16("Ubuntu\r\ndocker-desktop\r\nDebian\r\n")),
      ..Fake::default()
    }
  }

  fn names(profiles: &[Profile]) -> Vec<&str> {
    profiles
      .iter()
      .map(|profile| profile.name.as_str())
      .collect()
  }

  #[test]
  fn finds_the_windows_shells() {
    let fake = windows();
    let profiles = windows_profiles(&fake);
    assert_eq!(
      names(&profiles),
      [
        "Windows PowerShell",
        "Command Prompt",
        "Git Bash",
        "MSYS2",
        "Ubuntu (WSL)",
        "Debian (WSL)"
      ]
    );
    let ubuntu = &profiles[4].shell;
    assert_eq!(ubuntu.args, ["--distribution", "Ubuntu"]);
    assert!(ubuntu.program.ends_with("wsl.exe"));
    assert_eq!(profiles[2].shell.args, ["--login", "-i"]);
    assert!(
      profiles[3]
        .env
        .contains(&("CHERE_INVOKING".into(), "1".into()))
    );
    // Without PowerShell 7, Windows PowerShell is the default.
    assert_eq!(
      default_profile(&profiles, &fake).map(|p| p.name.as_str()),
      Some("Windows PowerShell")
    );
  }

  #[test]
  fn finds_powershell_7_on_the_path_or_its_newest_install() {
    let mut fake = windows();
    let install = PathBuf::from(r"C:\Program Files").join("PowerShell");
    fake
      .dirs
      .insert(install.clone(), vec![install.join("6"), install.join("7")]);
    fake.files.push(install.join("6").join("pwsh.exe"));
    fake.files.push(install.join("7").join("pwsh.exe"));
    let profiles = windows_profiles(&fake);
    assert_eq!(profiles[0].name, "PowerShell");
    assert_eq!(
      profiles[0].shell.program,
      install.join("7").join("pwsh.exe").display().to_string()
    );

    fake
      .path
      .insert("pwsh.exe", PathBuf::from(r"D:\tools\pwsh.exe"));
    assert_eq!(
      windows_profiles(&fake)[0].shell.program,
      r"D:\tools\pwsh.exe"
    );
    if cfg!(windows) {
      assert_eq!(
        default_profile(&windows_profiles(&fake), &fake).map(|p| p.name.as_str()),
        Some("PowerShell")
      );
    }
  }

  #[test]
  fn finds_git_bash_beside_git_on_the_path() {
    let mut fake = windows();
    fake
      .files
      .retain(|file| !file.to_string_lossy().contains("Git"));
    let scoop = PathBuf::from(r"D:\scoop\apps\git\current");
    fake
      .path
      .insert("git.exe", scoop.join("cmd").join("git.exe"));
    fake.files.push(scoop.join(r"bin\bash.exe"));
    let profiles = windows_profiles(&fake);
    let git_bash = profiles
      .iter()
      .find(|profile| profile.name == "Git Bash")
      .expect("found");
    assert_eq!(
      git_bash.shell.program,
      scoop.join(r"bin\bash.exe").display().to_string()
    );
  }

  #[test]
  fn reads_wsl_lists_in_utf16_or_utf8() {
    assert_eq!(
      parse_wsl_list(&utf16("\u{feff}Ubuntu-22.04\r\n\r\n")),
      ["Ubuntu-22.04"]
    );
    assert_eq!(
      parse_wsl_list(b"Ubuntu\ndocker-desktop-data\nkali-linux\n"),
      ["Ubuntu", "kali-linux"]
    );
    assert!(parse_wsl_list(b"").is_empty());
  }

  #[test]
  fn lists_unix_shells_once_each() {
    let fake = Fake {
      vars: HashMap::from([("SHELL", "/usr/bin/zsh")]),
      files: [
        "/bin/sh",
        "/bin/bash",
        "/usr/bin/bash",
        "/usr/bin/zsh",
        "/usr/bin/tmux",
      ]
      .iter()
      .map(PathBuf::from)
      .collect(),
      etc_shells: Some(
        "# /etc/shells\n/bin/sh\n/bin/bash\n/usr/bin/bash\n/usr/bin/zsh\n/usr/bin/tmux\n/usr/bin/fish\n",
      ),
      ..Fake::default()
    };
    let profiles = unix_profiles(&fake);
    // fish isn't installed; tmux isn't a shell; bash is listed twice.
    assert_eq!(names(&profiles), ["bash", "sh", "zsh"]);
    assert_eq!(profiles[0].shell.program, "/bin/bash");
    if cfg!(not(windows)) {
      assert_eq!(
        default_profile(&profiles, &fake).map(|p| p.name.as_str()),
        Some("zsh")
      );
    }
  }
}
