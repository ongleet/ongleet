import assert from 'node:assert/strict';
import test from 'node:test';
import { violations } from './check-system-boundaries.mjs';
const feature = 'src-tauri/src/session.rs';
test('test fixtures cannot hide native production access after raw strings', () => {
  const source =
    '#[cfg(test)] mod tests { const FIXTURE: &str = r###"} { // gtk::"###; }\nuse std::os::unix::net::UnixStream;';
  assert.equal(violations(source, feature).length, 1);
  assert.deepEqual(
    violations('#[cfg(test)] fn fixture() { gtk::init(); }\nfn production() {}', feature),
    [],
  );
});
test('native calls, grouped imports, paths and feature factories are rejected', () => {
  for (const source of [
    'use std::{os::fd::OwnedFd};',
    'use std::{os::{unix::{net::UnixStream}}};',
    'unsafe { libc::setsid(); }',
    'use windows_sys::Win32::System::Console;',
    'use windows::Win32::System::Console;',
    'use std::os::windows::io::AsRawHandle;',
    'window.hwnd();',
    'window.gtk_window();',
    'window.ns_window();',
    'use objc2_app_kit::NSPasteboard;',
    'use security_framework::passwords;',
    'use objc2_foundation::NSString;',
    'use objc2_web_kit::WKWebView;',
    'use core_foundation::data::CFData;',
    'use std::{process::{Command as NativeCommand}};',
    'let image = format!("/proc/{}/exe", id);',
    'let runtime = Path::new("/run/user");',
    'let stat = r###"/proc/self/stat"###;',
    'let system = crate::system::services();',
    'use crate::system::{services as native};',
    '#[cfg(target_os = "linux")] mod feature;',
  ])
    assert.ok(violations(source, feature).length > 0, source);
  assert.deepEqual(violations('let system = system::services();', 'src-tauri/src/main.rs'), []);
  assert.deepEqual(
    violations(
      'use std::fs; // gtk:: is a comment\nfn f() { std::process::id(); let pairs = bytes.windows(2); }',
      feature,
    ),
    [],
  );
});

test('Windows paths retain backend selection and entrypoint restrictions', () => {
  assert.deepEqual(violations('let system = system::services();', 'src-tauri\\src\\main.rs'), []);
  assert.deepEqual(
    violations('let system = system::services();', 'src-tauri\\src\\bin\\ongleet.rs'),
    [],
  );
  assert.ok(
    violations('let system = system::services();', 'src-tauri\\src\\session.rs').length > 0,
  );
  assert.deepEqual(
    violations('#[cfg(target_os = "windows")] mod windows;', 'src-tauri\\src\\system\\mod.rs'),
    [],
  );
});
