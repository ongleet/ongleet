import { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TerminalFrontend, type ProcessStatus } from './terminal';
import './style.css';

function App() {
  const host = useRef<HTMLDivElement>(null);
  const frontend = useRef<TerminalFrontend | null>(null);
  const [cwd, setCwd] = useState('');
  const [executable, setExecutable] = useState('');
  const [args, setArgs] = useState('[]');
  const [status, setStatus] = useState<ProcessStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [snapshot, setSnapshot] = useState<string | null>(null);

  const report = (error: unknown) => setError(String(error));
  useEffect(() => {
    if (!isTauri()) { setError('Native backend unavailable. Start with npm run tauri dev.'); return; }
    const terminal = new TerminalFrontend(host.current!, setStatus, report);
    frontend.current = terminal;
    void invoke<[string, string]>('defaults').then(([home, shell]) => { setCwd(home); setExecutable(shell); }).catch(report);
    // This prototype closes and terminates; hide/reopen belongs to stage four.
    let disposed = false;
    const unlisten = getCurrentWindow().onCloseRequested(async event => {
      event.preventDefault();
      if (!window.confirm('Quit the prototype and terminate its shell and terminal jobs?')) return;
      try { await terminal.close(); await getCurrentWindow().destroy(); } catch (error) { report(error); }
    });
    void unlisten.then(off => { if (disposed) off(); });
    return () => { disposed = true; void unlisten.then(off => off()); terminal.dispose(); };
  }, []);

  async function launch(codex = false) {
    if (!frontend.current) return;
    setBusy(true); setError(''); setSnapshot(null);
    try {
      const parsed: unknown = JSON.parse(args);
      if (!Array.isArray(parsed) || !parsed.every(arg => typeof arg === 'string')) throw new Error('Arguments must be a JSON array of strings.');
      await frontend.current.start({ cwd, executable: codex ? 'codex' : executable || null, args: codex ? [] : parsed, env: {} });
    } catch (error) { report(error); } finally { setBusy(false); }
  }

  return <main>
    <header><strong>Terminal stack prototype</strong><span>Ghostty WASM · Tauri 2 · portable-pty</span>
      <output aria-live="polite">{status?.state === 'exited' ? `Exited (${status.code})` : status ? 'Running' : 'Stopped'}</output></header>
    <form onSubmit={event => { event.preventDefault(); void launch(); }}>
      <label>Working directory<input value={cwd} onChange={event => setCwd(event.target.value)} disabled={!!status || busy} required /></label>
      <label>Executable<input value={executable} onChange={event => setExecutable(event.target.value)} disabled={!!status || busy} placeholder="User shell" /></label>
      <label>Arguments (JSON array)<input value={args} onChange={event => setArgs(event.target.value)} disabled={!!status || busy} /></label>
      <button disabled={!!status || busy || !cwd}>Start</button>
      <button type="button" disabled={!!status || busy || !cwd} onClick={() => void launch(true)}>Run Codex</button>
    </form>
    <nav aria-label="Terminal actions">
      <button disabled={!status || busy} onClick={() => void frontend.current?.copy().catch(report)}>Copy selection</button>
      <button disabled={!status || busy} onClick={() => void frontend.current?.paste().catch(report)}>Paste</button>
      <button disabled={!status || busy} onClick={() => { try { setSnapshot(frontend.current!.snapshot()); } catch (error) { report(error); } }}>Inspect screen</button>
      <button disabled={!status || busy} onClick={() => {
        if (!window.confirm('Terminate this terminal session and its jobs?')) return;
        setBusy(true);
        void frontend.current?.close().catch(report).finally(() => setBusy(false));
      }}>Close session</button>
    </nav>
    {error && <div role="alert" className="error">{error}</div>}
    <div ref={host} className="terminal" />
    {snapshot !== null && <section aria-label="Plain-text snapshot"><button onClick={() => setSnapshot(null)}>Dismiss snapshot</button><pre>{snapshot}</pre></section>}
    <footer>Validation pending · Ctrl+Shift+C / V: copy / paste · Commands run directly; for shell syntax explicitly launch your shell with ["-lc", "…"].</footer>
  </main>;
}

// A single mounted owner keeps terminal instances independent of React renders.
createRoot(document.getElementById('root')!).render(<App />);
