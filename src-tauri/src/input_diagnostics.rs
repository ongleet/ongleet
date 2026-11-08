use tauri::Manager;
pub fn install(app: &tauri::App) -> tauri::Result<()> {
    if !crate::diagnostics::enabled() {
        return Ok(());
    }
    app.state::<crate::system::SystemServices>()
        .desktop
        .install_input_diagnostics(app, std::sync::Arc::new(record))
        .map_err(std::io::Error::other)?;
    Ok(())
}

fn record(event: crate::system::NativeInputDiagnostic) {
    use crate::system::NativeInputDiagnostic;
    match event {
        NativeInputDiagnostic::Enabled {
            label,
            active,
            visible,
        } => eprintln!("[native-input] enabled window={label:?} active={active} visible={visible}"),
        NativeInputDiagnostic::Event(event) => {
            let crate::system::NativeInputEvent {
                elapsed_ms,
                label,
                event,
                active,
                focused,
                visible,
                send_event,
                key,
            } = event;
            if let Some(key) = key {
                eprintln!("[native-input] elapsed_ms={elapsed_ms} window={label:?} event={event} active={active} focused={focused} visible={visible} native_time={} hardware_code={} keyval={} group={} modifiers={} send_event={send_event}", key.time, key.hardware_code, key.keyval, key.group, key.modifiers);
            } else {
                eprintln!("[native-input] elapsed_ms={elapsed_ms} window={label:?} event=focus active={active} focused={focused} visible={visible} send_event={send_event}");
            }
        }
    }
}
