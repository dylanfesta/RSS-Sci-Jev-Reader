// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use smart_rss_reader_lib::agent_cli::{run_agent_cli_from_args, CliArgs};

fn main() {
    let args = CliArgs::parse();
    if args.resource.is_some() {
        let code = run_agent_cli_from_args(args);
        std::process::exit(code);
    }
    smart_rss_reader_lib::run_tauri_app(args.data_dir);
}
