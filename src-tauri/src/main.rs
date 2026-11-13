// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = ide_run::run_riscv_helper() {
        std::process::exit(code);
    }
    // Started by `ssh` / `git` as their askpass program: answer and exit, no UI.
    if let Some(code) = ide_ssh::askpass::run_helper() {
        std::process::exit(code);
    }
    openidea_lib::run()
}
