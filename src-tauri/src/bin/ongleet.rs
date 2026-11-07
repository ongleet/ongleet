#[allow(dead_code)]
#[path = "../system/mod.rs"]
mod system;
// The CLI dispatches protocol calls without initializing desktop or metadata state.
#[path = "../cli/mod.rs"]
mod cli;
#[allow(dead_code)]
#[path = "../control/mod.rs"]
mod control;
fn main() {
    std::process::exit(cli::run(&system::services()));
}
