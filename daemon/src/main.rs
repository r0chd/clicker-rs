mod config;

use clap::Parser;
use env_logger::Builder;
use log::LevelFilter;
use std::path::PathBuf;

#[derive(clap::ValueEnum, Clone, Debug)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(
        long,
        value_enum,
        default_value_t = LogLevel::Info,
        help = "Set the log level"
    )]
    log_level: LogLevel,

    #[arg(short, long, value_name = "FILE", help = "Path to the config file")]
    config: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let log_level = match cli.log_level {
        LogLevel::Error => LevelFilter::Error,
        LogLevel::Warn => LevelFilter::Warn,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Debug => LevelFilter::Debug,
        LogLevel::Trace => LevelFilter::Trace,
    };

    Builder::new()
        .filter(Some("daemon"), log_level)
        .init();

    let config = config::Config::load(cli.config);
    println!("{:?}", config);
}
