// Release builds use the GUI subsystem so Windows does not open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  // Run by Git or SSH to ask for a password: ask the running editor.
  if let Some(code) = ferrite_extension_host::askpass::client_main() {
    std::process::exit(code);
  }
  ferrite_app::run();
}
