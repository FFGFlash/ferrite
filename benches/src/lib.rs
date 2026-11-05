//! Deterministic fixtures for Flareite's benchmarks: code-like text of a given
//! size and synthetic workspaces on disk. Generated rather than checked in,
//! so the repository stays small and every run uses identical inputs.

use std::fs;
use std::path::Path;

/// A small deterministic pseudo-random generator (64-bit LCG).
pub struct Lcg(u64);

impl Lcg {
  pub fn new(seed: u64) -> Self {
    Self(seed)
  }

  pub fn next_u64(&mut self) -> u64 {
    self.0 = self
      .0
      .wrapping_mul(6_364_136_223_846_793_005)
      .wrapping_add(1_442_695_040_888_963_407);
    self.0 >> 33
  }

  pub fn below(&mut self, limit: u64) -> u64 {
    self.next_u64() % limit.max(1)
  }
}

const WORDS: &[&str] = &[
  "let",
  "fn",
  "match",
  "value",
  "buffer",
  "index",
  "result",
  "self",
  "return",
  "if",
  "else",
  "for",
  "while",
  "editor",
  "document",
  "line",
  "offset",
  "range",
  "selection",
  "cursor",
];

/// One line of code-like text, indented, around `width` bytes.
pub fn code_line(rng: &mut Lcg, width: usize) -> String {
  let mut line = " ".repeat(2 * rng.below(4) as usize);
  while line.len() < width {
    line.push_str(WORDS[rng.below(WORDS.len() as u64) as usize]);
    line.push_str(match rng.below(6) {
      0 => "(",
      1 => ") ",
      2 => ".",
      3 => " = ",
      _ => " ",
    });
  }
  line.push(';');
  line
}

/// About `bytes` of code-like text with LF line endings.
pub fn code_text(bytes: usize) -> String {
  let mut rng = Lcg::new(42);
  let mut text = String::with_capacity(bytes + 128);
  while text.len() < bytes {
    let width = 40 + rng.below(60) as usize;
    text.push_str(&code_line(&mut rng, width));
    text.push('\n');
  }
  text
}

/// A workspace of `files` source files (about `file_bytes` each) in nested
/// directories, plus a `.gitignore`d `target/` directory and a `.git` marker.
pub fn create_workspace(root: &Path, files: usize, file_bytes: usize) {
  let mut rng = Lcg::new(7);
  fs::create_dir_all(root.join(".git")).expect("create .git");
  fs::write(root.join(".gitignore"), "target/\n").expect("write .gitignore");
  for i in 0..files {
    let dir = root.join(format!("src/module_{}/sub_{}", i / 100, (i / 10) % 10));
    fs::create_dir_all(&dir).expect("create directory");
    let mut text = String::with_capacity(file_bytes + 128);
    while text.len() < file_bytes {
      text.push_str(&code_line(&mut rng, 60));
      text.push('\n');
    }
    if i % 50 == 0 {
      text.push_str("// needle: a rare marker\n");
    }
    fs::write(dir.join(format!("file_{i}.rs")), text).expect("write file");
  }
  let target = root.join("target/debug");
  fs::create_dir_all(&target).expect("create target");
  for i in 0..files / 10 {
    fs::write(target.join(format!("artifact_{i}.o")), "needle ignored").expect("write artifact");
  }
}

/// `count` synthetic relative paths, like a large repository's file list.
pub fn synthetic_paths(count: usize) -> Vec<String> {
  let mut rng = Lcg::new(3);
  (0..count)
    .map(|i| {
      let depth = 1 + rng.below(4);
      let mut path = String::new();
      for _ in 0..depth {
        path.push_str(WORDS[rng.below(WORDS.len() as u64) as usize]);
        path.push('/');
      }
      path.push_str(&format!(
        "{}_{i}.rs",
        WORDS[rng.below(WORDS.len() as u64) as usize]
      ));
      path
    })
    .collect()
}
