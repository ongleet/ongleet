#[path = "../codex.rs"]
mod codex;
#[allow(dead_code)]
#[path = "../control/mod.rs"]
mod control;
#[allow(dead_code)]
#[path = "../control/paths.rs"]
mod paths;
#[allow(dead_code)]
#[path = "../system/mod.rs"]
mod system;

fn main() {
    let services = system::services();
    if codex::run_if_requested(&services) {
        return;
    }
    let result = (|| -> Result<(), String> {
        let mut args: Vec<_> = std::env::args().skip(1).collect();
        // Explicit IDs also allow use from a shell or a custom action command.
        while let Some(key) = args.first().map(String::as_str) {
            let env = match key {
                "--session-id" | "--workspace-id" => "ONGLEET_SESSION_ID",
                "--tab-id" => "ONGLEET_TAB_ID",
                _ => break,
            };
            if args.len() < 2 {
                return Err(format!("{} requires a value", args[0]));
            }
            std::env::set_var(env, &args[1]);
            args.drain(..2);
        }
        if std::env::var_os("ONGLEET_CODEX_DATA").is_none() {
            std::env::set_var(
                "ONGLEET_CODEX_DATA",
                services.paths.data_directory(paths::APPLICATION_ID)?,
            );
        }
        std::env::set_var(
            "ONGLEET_CODEX_HOOK_EXE",
            services.paths.application_executable()?,
        );
        codex::run(&services, codex::WRAPPER_FLAG, args);
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
