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
    'window.gtk_window();',
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
    violations('use std::fs; // gtk:: is a comment\nfn f() { std::process::id(); }', feature),
    [],
  );
});
