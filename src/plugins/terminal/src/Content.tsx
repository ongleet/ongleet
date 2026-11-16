import { t } from '../../../i18n';
import { TabPanel } from '../../../components/Controls';
import { useSdk, useTabInteractions, SdkError } from '../../../sdk';
import { useEffect, useRef, useState } from 'react';
import { TerminalFrontend, type ProcessStatus } from './frontend';
import type { PluginTab } from '../../types';
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
  const [status, setStatus] = useState<ProcessStatus | null>(null);
  const [starting, setStarting] = useState(true);
  const [interactionReady, setInteractionReady] = useState(false);
  const [slow, setSlow] = useState(false);
  const inputActions = useRef<Promise<unknown>>(Promise.resolve());
  const queuedActions = useRef(0);
  useTabInteractions(
    {
      state: (args) => {
        if (!frontend.current) throw new SdkError('unavailable', 'Terminal unavailable');
        return frontend.current.interactionState(args.lines as number | undefined);
      },
      actions: {
        'send-text': (args, { signal }) => {
          if (queuedActions.current >= 4)
            throw new SdkError('limit_exceeded', 'Terminal interaction queue exceeds limit');
          queuedActions.current++;
          const task = inputActions.current
            .catch(() => {})
            .then(() => {
              if (signal.aborted) throw new SdkError('cancelled', 'Interaction cancelled');
              if (!frontend.current) throw new SdkError('unavailable', 'Terminal unavailable');
              return frontend.current.sendText(
                args.text as string,
                args.enter as boolean | undefined,
              );
            })
            .finally(() => {
              queuedActions.current--;
            });
          inputActions.current = task;
          return task;
        },
      },
    },
    interactionReady,
  );
  const report: Report = (error) => setError(String(error));

  useEffect(() => {
    const terminal = new TerminalFrontend(
      host.current!,
      tab.id,
      (status) => {
        setStatus(status);
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
    setInteractionReady(false);
    setSlow(false);
    markRestoring(1);
    const timer = setTimeout(() => {
      if (!disposed) setSlow(true);
    }, 10_000);
    void terminal
      .start(tab.resumeOnStart)
      .then(() => {
        if (!disposed) setInteractionReady(true);
      })
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
    <TabPanel
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      className="tab-terminal"
      hidden={!visible}
      aria-busy={starting}
    >
      {starting && (
        <div role="status" className="terminal-startup">
          {slow
            ? t(
                'Terminal startup is taking longer than expected. Other tabs and sessions remain available.',
              )
            : t('Starting terminal…')}
        </div>
      )}
      {error && (
        <div role="alert" className="error">
          {error}
        </div>
      )}
      <div ref={host} className="terminal" />
      <footer className="plugin-statusbar" aria-label={t('Terminal status')}>
        <span>
          {starting
            ? t('Starting…')
            : status?.state === 'exited'
              ? t('Exited ({value0})', { value0: status.code })
              : status?.state === 'running'
                ? t('Running')
                : t('Stopped')}
        </span>
        <span>{t('Ctrl+Shift+C / V: copy / paste')}</span>
        <span>{t('Arrow keys on tab bar: navigate tabs')}</span>
      </footer>
    </TabPanel>
  );
}
