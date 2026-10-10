//! One binary, two front ends: a window when it is opened like an application, the pipeline
//! when it is given arguments. Neither is the real one. Anything a person can do in the
//! window can be scripted, and anything a script can do can be watched happening.

// A console program on Windows opens a console window beside the app for as long as it runs.
// As a windowed program the pipeline still writes to whatever pipe or file it is given, which
// is how a terminal (Git Bash by itself, PowerShell through `| Out-Host`) shows its output. Debug
// builds keep the console, for `cargo run`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod cli;
mod mcp;
mod ui;
mod update;

/// Enough for the argument parser to build itself without inlining. Every subcommand and every
/// argument is a nested builder call, so an unoptimised build walks far deeper than Windows
/// gives the main thread by default, and `cargo run -- --version` overflows before reaching any
/// command at all. Reserved address space, not memory: nothing is paid unless it is touched.
const CLI_STACK: usize = 32 * 1024 * 1024;

fn main() -> anyhow::Result<()> {
    let first = std::env::args_os().nth(1);
    if first.as_deref() == Some(mcp::WITHOUT_WINDOW.as_ref()) {
        return ui::run(true);
    }
    if first.is_some() {
        // The window has to stay on the main thread; the pipeline does not, so only it moves.
        return std::thread::Builder::new()
            .stack_size(CLI_STACK)
            .name("steamgauge-cli".to_owned())
            .spawn(|| tokio::runtime::Runtime::new()?.block_on(cli::run()))?
            .join()
            .map_err(|_| anyhow::anyhow!("the command panicked"))?;
    }
    ui::run(false)
}
