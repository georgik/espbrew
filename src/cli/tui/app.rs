use crate::cli::args::Cli;
use anyhow::Result;

pub struct TuiApp {
    _cli: Cli,
}

impl TuiApp {
    pub fn new(cli: Cli) -> Result<Self> {
        Ok(Self { _cli: cli })
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("🖥️  TUI mode - TODO: implement");
        println!("⚠️  Using CLI mode for now");
        Ok(())
    }
}
