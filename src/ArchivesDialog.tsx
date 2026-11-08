import { t, getLocale } from './i18n';
import { Button, Dialog } from './components/Controls';
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

type Archive = {
  id: number;
  kind: 'session' | 'tab';
  name: string;
  sessionName: string;
  projectName: string;
  archivedAt: number;
  expiresAt: number;
  purging: boolean;
  restartTerminals: number;
};
type ArchiveList = { entries: Archive[]; cleanupError: string | null };
export function ArchivesDialog({
  dismiss,
  changed,
  confirm,
}: {
  dismiss: () => void;
  changed: () => Promise<void>;
  confirm: (message: string) => Promise<boolean>;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [data, setData] = useState<ArchiveList>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  async function refresh() {
    try {
      setData(await invoke<ArchiveList>('archives'));
    } catch (error) {
      setError(String(error));
    }
  }
  useEffect(() => {
    dialog.current?.showModal();
    void refresh();
  }, []);
  async function act(command: string, args: Record<string, unknown>) {
    setBusy(true);
    setError('');
    try {
      await invoke(command, args);
      await changed();
      await refresh();
    } catch (error) {
      setError(String(error));
      await refresh();
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog
      ref={dialog}
      aria-label={t('Archives')}
      className="archives-dialog"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) dismiss();
      }}
    >
      <div className="archives-heading">
        <h2>{t('Archives')}</h2>
        <Button disabled={busy} onClick={dismiss}>
          {t('Close')}
        </Button>
      </div>
      <p className="muted">
        {t(
          'Sessions and removed tabs are kept for 30 days from archival. Expired archives are purged at startup and when archiving. Working files are never deleted.',
        )}
      </p>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {data?.cleanupError && (
        <p className="error" role="alert">
          {t('Archive cleanup needs attention: ')}
          {data.cleanupError}
        </p>
      )}
      {!data && !error && <p role="status">{t('Loading archives…')}</p>}
      {data?.entries.length === 0 && <p>{t('No archives.')}</p>}
      {!!data?.entries.length && (
        <table className="archives-table">
          <thead>
            <tr>
              <th>{t('Name')}</th>
              <th>{t('Type / context')}</th>
              <th>{t('Archived')}</th>
              <th>{t('Expires')}</th>
              <th>{t('Actions')}</th>
            </tr>
          </thead>
          <tbody>
            {data.entries.map((entry) => (
              <tr key={entry.id}>
                <td>
                  {entry.name}
                  <small>
                    {t('Archive #')}
                    {entry.id}
                  </small>
                </td>
                <td>
                  {entry.kind === 'session' ? t('Session') : t('Tab')}
                  <small>
                    {entry.projectName}
                    {entry.kind === 'tab' ? ` / ${entry.sessionName}` : ''}
                  </small>
                </td>
                <td>{new Date(entry.archivedAt * 1000).toLocaleDateString(getLocale())}</td>
                <td>
                  {entry.purging
                    ? t('Deletion pending')
                    : new Date(entry.expiresAt * 1000).toLocaleDateString(getLocale())}
                </td>
                <td>
                  <div className="archive-actions">
                    <Button
                      disabled={busy || entry.purging || entry.expiresAt * 1000 <= Date.now()}
                      onClick={async () => {
                        if (
                          entry.restartTerminals > 0 &&
                          !(await confirm(
                            t(
                              'Restore “{value0}” and launch {value1} previously running terminal command(s) in new processes?',
                              { value0: entry.name, value1: entry.restartTerminals },
                            ),
                          ))
                        )
                          return;
                        await act('restore_archive', { id: entry.id });
                      }}
                    >
                      {t('Restore')}
                    </Button>
                    <Button
                      disabled={busy}
                      onClick={async () => {
                        if (
                          await confirm(
                            t(
                              'Permanently delete archive “{value0}” and its Ongleet history? This cannot be undone. Working files stay untouched.',
                              { value0: entry.name },
                            ),
                          )
                        )
                          await act('purge_archive', { id: entry.id, confirmed: true });
                      }}
                    >
                      {t('Delete permanently')}
                    </Button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="dialog-actions">
        <Button
          disabled={busy}
          onClick={async () => {
            if (
              await confirm(
                t('Permanently purge archives older than 30 days and retry pending cleanup?'),
              )
            )
              await act('prune_archives', { confirmed: true });
          }}
        >
          {t('Purge expired')}
        </Button>
      </div>
    </Dialog>
  );
}
