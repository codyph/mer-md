#![recursion_limit = "256"]

mod cli;
mod console;
mod document;
mod project;
mod render;

use std::process::ExitCode;

fn main() -> ExitCode {
    let exit_code = match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    };
    console::pause_on_exit();
    exit_code
}

fn run() -> anyhow::Result<()> {
    match cli::parse_args(std::env::args_os().skip(1))? {
        Some(options) => project::execute(&options),
        None => {
            print!("{}", cli::HELP);
            Ok(())
        }
    }
}
