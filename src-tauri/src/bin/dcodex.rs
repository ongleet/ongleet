#[path = "../codex.rs"]
mod codex;
#[allow(dead_code)]
#[path = "../control/mod.rs"]
mod control;
#[allow(dead_code)]
#[path = "../control/paths.rs"]
mod paths;

fn main() {
    if codex::run_if_requested() {
        return;
    }
    let result = (|| -> Result<(), String> {
        let mut args: Vec<_> = std::env::args().skip(1).collect();
        // Explicit IDs also allow use from a shell or a custom action command.
        while let Some(key) = args.first().map(String::as_str) {
            let env = match key {
                "--session-id" | "--workspace-id" => "DEVCENTER_SESSION_ID",
                "--tab-id" => "DEVCENTER_TAB_ID",
                _ => break,
            };
            if args.len() < 2 {
                return Err(format!("{} requires a value", args[0]));
            }
            std::env::set_var(env, &args[1]);
            args.drain(..2);
        }
        if std::env::var_os("DEVCENTER_CODEX_DATA").is_none() {
            std::env::set_var(
                "DEVCENTER_CODEX_DATA",
                paths::data_directory(paths::APPLICATION_ID)?,
            );
        }
        std::env::set_var(
            "DEVCENTER_CODEX_HOOK_EXE",
            std::env::current_exe().map_err(|e| e.to_string())?,
        );
        codex::run(codex::WRAPPER_FLAG, args);
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
