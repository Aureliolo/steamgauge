mod cli;
mod ui;

/// One binary, two front ends: a window when it is opened like an application, the pipeline
/// when it is given arguments. Neither is the real one. Anything a person can do in the
/// window can be scripted, and anything a script can do can be watched happening.
/// Enough for the argument parser to build itself without inlining. Every subcommand and every
/// argument is a nested builder call, so an unoptimised build walks far deeper than Windows
/// gives the main thread by default, and `cargo run -- --version` overflows before reaching any
/// command at all. Reserved address space, not memory: nothing is paid unless it is touched.
const CLI_STACK: usize = 32 * 1024 * 1024;

fn main() -> anyhow::Result<()> {
    if std::env::args_os().nth(1).is_some() {
        // The window has to stay on the main thread; the pipeline does not, so only it moves.
        return std::thread::Builder::new()
            .stack_size(CLI_STACK)
            .name("steamgauge-cli".to_owned())
            .spawn(|| tokio::runtime::Runtime::new()?.block_on(cli::run()))?
            .join()
            .map_err(|_| anyhow::anyhow!("the command panicked"))?;
    }
    ui::run()
}
