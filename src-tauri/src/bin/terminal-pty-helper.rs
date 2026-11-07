#[path = "../codex.rs"]
#[allow(dead_code)]
mod codex;
#[allow(dead_code)]
#[path = "../system/mod.rs"]
mod system;

#[allow(dead_code)]
#[path = "../control/mod.rs"]
mod control;
fn main() {
    let services = system::services();
    if codex::run_if_requested(&services) {
        return;
    }
    if !system::run_helper_if_requested() {
        std::process::exit(2);
    }
}
