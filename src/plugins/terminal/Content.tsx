import { useSdk } from '../../sdk';
import { useEffect, useRef, useState } from 'react';
import { TerminalFrontend, type ProcessStatus } from './frontend';
import type { PluginTab } from '../types';
type Report = (error: unknown) => void;

export function TerminalContent({
  tab,
  visible,
  reportStatus,
  markRestoring,
}: {
  tab: PluginTab;
  visible: boolean;
  reportStatus: (id: number, status: ProcessStatus | null) => void;
  markRestoring: (delta: number) => void;
}) {
  const sdk = useSdk();
  const host = useRef<HTMLDivElement>(null);
  const frontend = useRef<TerminalFrontend | null>(null);
  const [error, setError] = useState('');
  const [starting, setStarting] = useState(true);
  const [slow, setSlow] = useState(false);
  const report: Report = (error) => setError(String(error));

  useEffect(() => {
    const terminal = new TerminalFrontend(
      host.current!,
      tab.id,
      (status) => {
        reportStatus(tab.id, status);
      },
      report,
      sdk,
    );
    frontend.current = terminal;
    // Reconnect live/exited sessions after a WebView reload. Only eligible
    // stopped configurations may launch a new process.
    let attached = false;
    let disposed = false;
    const finishAttachment = () => {
      if (attached) return;
      attached = true;
      markRestoring(-1);
    };
    setStarting(true);
    setSlow(false);
    markRestoring(1);
    const timer = setTimeout(() => {
      if (!disposed) setSlow(true);
    }, 10_000);
    void terminal
      .start(tab.resumeOnStart)
      .catch((error) => {
        if (!disposed) report(error);
      })
      .finally(() => {
        clearTimeout(timer);
        finishAttachment();
        if (!disposed) setStarting(false);
      });
    return () => {
      disposed = true;
      clearTimeout(timer);
      terminal.dispose();
      frontend.current = null;
      finishAttachment();
    };
  }, [tab.id, reportStatus, markRestoring]);
  useEffect(() => {
    frontend.current?.setVisible(visible);
  }, [visible]);

  return (
    <section
      id={`tab-panel-${tab.id}`}
      role="tabpanel"
      aria-labelledby={`tab-${tab.id}`}
      className="tab-terminal"
      hidden={!visible}
      aria-busy={starting}
    >
      {starting && (
        <div role="status" className="terminal-startup">
          {slow
            ? 'Terminal startup is taking longer than expected. Other tabs and sessions remain available.'
            : 'Starting terminal…'}
        </div>
      )}
      {error && (
        <div role="alert" className="error">
          {error}
        </div>
      )}
      <div ref={host} className="terminal" />
    </section>
  );
}
