// A console program on Windows opens a console window beside the app for as long as it runs.
// As a windowed program the pipeline still writes to whatever pipe or file it is given, which
// is how a terminal (Git Bash by itself, PowerShell through `| Out-Host`) shows its output. Debug
// builds keep the console, for `cargo run`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    steamgauge_app::run()
}
