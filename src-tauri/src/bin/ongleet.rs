// The CLI links the socket protocol/client only; it does not open SQLite or GTK.
#[path = "../cli/mod.rs"]
mod cli;
#[allow(dead_code)]
#[path = "../control/mod.rs"]
mod control;
fn main() {
    std::process::exit(cli::run());
}
