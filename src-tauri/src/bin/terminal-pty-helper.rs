#[path = "../codex.rs"]
#[allow(dead_code)]
mod codex;
#[path = "../containment.rs"]
#[allow(dead_code)]
mod containment;
fn main() {
    if codex::run_if_requested() {
        return;
    }
    if !containment::run_helper_if_requested() {
        std::process::exit(2);
    }
}
