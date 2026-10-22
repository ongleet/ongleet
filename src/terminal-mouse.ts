import type { Terminal } from 'ghostty-web';

// Ghostty owns mode state; this adapter supplies the missing browser events.
export function attachTerminalMouse(host: HTMLElement, term: Terminal, send: (bytes: Uint8Array) => void, enabled: () => boolean) {
  const canvas = host.querySelector('canvas')!;
  let pressed: number | undefined;
  let lastMotion = '';
  const report = (event: MouseEvent, button: number, release = false) => {
    const rect = canvas.getBoundingClientRect();
    const x = Math.max(1, Math.min(term.cols, Math.floor((event.clientX - rect.left) / term.renderer!.charWidth) + 1));
    const y = Math.max(1, Math.min(term.rows, Math.floor((event.clientY - rect.top) / term.renderer!.charHeight) + 1));
    const modifiers = (event.shiftKey ? 4 : 0) | (event.altKey ? 8 : 0) | (event.ctrlKey ? 16 : 0);
    const code = button | modifiers;
    if (term.getMode(1006)) send(new TextEncoder().encode(`\x1b[<${code};${x};${y}${release ? 'm' : 'M'}`));
    else if (x <= 223 && y <= 223) send(new Uint8Array([27, 91, 77, 32 + (release ? 3 | modifiers : code), x + 32, y + 32]));
    return `${code}:${x}:${y}`;
  };
  const active = (event: MouseEvent) => enabled() && !event.shiftKey && !event.ctrlKey && !event.metaKey && term.hasMouseTracking();
  const down = (event: MouseEvent) => {
    if (!active(event) || event.button > 2) return;
    event.preventDefault(); event.stopImmediatePropagation(); term.focus();
    pressed = event.button === 1 ? 1 : event.button === 2 ? 2 : 0;
    lastMotion = ''; report(event, pressed);
  };
  const move = (event: MouseEvent) => {
    if (!active(event)) return;
    event.preventDefault(); event.stopImmediatePropagation();
    if (!term.getMode(1003) && !(pressed !== undefined && term.getMode(1002))) return;
    // Browser mousemove frequency is bounded by the existing input queue.
    const rect = canvas.getBoundingClientRect();
    const key = `${event.buttons}:${Math.floor((event.clientX - rect.left) / term.renderer!.charWidth)}:${Math.floor((event.clientY - rect.top) / term.renderer!.charHeight)}`;
    if (lastMotion !== key) { report(event, (pressed ?? 3) | 32); lastMotion = key; }
  };
  const up = (event: MouseEvent) => {
    if (pressed === undefined) return;
    if (enabled() && term.hasMouseTracking() && !term.getMode(9)) report(event, pressed, true);
    pressed = undefined; lastMotion = '';
    event.preventDefault(); event.stopImmediatePropagation();
  };
  const wheel = (event: WheelEvent) => {
    if (!active(event) || !event.deltaY) return;
    event.preventDefault(); event.stopImmediatePropagation();
    const count = Math.min(5, Math.max(1, Math.ceil(Math.abs(event.deltaY) / (event.deltaMode === 0 ? 40 : 1))));
    for (let i = 0; i < count; i++) report(event, event.deltaY < 0 ? 64 : 65);
  };
  host.addEventListener('mousedown', down, true);
  host.addEventListener('mousemove', move, true);
  document.addEventListener('mouseup', up, true);
  host.addEventListener('wheel', wheel, { capture: true, passive: false });
  return { dispose() {
    host.removeEventListener('mousedown', down, { capture: true });
    host.removeEventListener('mousemove', move, { capture: true });
    document.removeEventListener('mouseup', up, { capture: true });
    host.removeEventListener('wheel', wheel, { capture: true });
  } };
}
