use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "git-explain",
    about = "Read and understand code changed in the working tree"
)]
pub struct Cli {
    #[arg(long, help = "Print changed Rust functions and exit")]
    pub debug: bool,
    #[arg(long, default_value_t = 8081, help = "Local web-server port")]
    pub port: u16,
}
