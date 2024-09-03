use clap::Parser;

lazy_static::lazy_static! {
    pub(crate) static ref ARGS: Args =
        Args::parse();
}

/// This service act as a plugin for Gaia domain.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Whether to start with a cluster
    #[arg(short, long)]
    pub cluster: bool,
}
