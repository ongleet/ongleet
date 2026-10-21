#[path = "../containment.rs"]
#[allow(dead_code)]
mod containment;
fn main() {
    if !containment::run_helper_if_requested() {
        std::process::exit(2);
    }
}
