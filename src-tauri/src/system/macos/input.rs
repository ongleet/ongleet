//! Main-thread local AppKit event monitor; observes without consuming keys.
use super::super::{NativeInputDiagnostic, NativeInputEvent, NativeKey};
use objc2_app_kit::{NSEvent, NSEventMask};
use std::{ptr::NonNull, sync::Arc};
pub fn install(
    label: String,
    record: Arc<dyn Fn(NativeInputDiagnostic) + Send + Sync>,
    state: impl Fn() -> (bool, bool) + 'static,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    let monitor = block2::RcBlock::new(move |pointer: NonNull<NSEvent>| {
        let event = unsafe { pointer.as_ref() };
        let (focused, visible) = state();
        record(NativeInputDiagnostic::Event(NativeInputEvent {
            elapsed_ms: start.elapsed().as_millis(),
            label: label.clone(),
            event: format!("{:?}", event.r#type()),
            active: focused,
            focused,
            visible,
            send_event: false,
            key: Some(NativeKey {
                time: (event.timestamp() * 1000.0) as u32,
                hardware_code: event.keyCode(),
                keyval: if event.r#type() == objc2_app_kit::NSEventType::FlagsChanged {
                    String::new()
                } else {
                    event
                        .charactersIgnoringModifiers()
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                },
                group: 0,
                modifiers: format!("{:?}", event.modifierFlags()),
            }),
        }));
        pointer.as_ptr()
    });
    let token = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::KeyDown | NSEventMask::KeyUp | NSEventMask::FlagsChanged,
            &monitor,
        )
    }
    .ok_or("Could not install native input monitor")?;
    // AppKit owns the copied block. Keep its token on the main thread for
    // this process lifetime; no global keyboard/accessibility permission.
    INPUT_MONITORS.with(|tokens| tokens.borrow_mut().push(token));
    Ok(())
}

thread_local! {static INPUT_MONITORS:std::cell::RefCell<Vec<objc2::rc::Retained<objc2::runtime::AnyObject>>>=const{std::cell::RefCell::new(Vec::new())};}
