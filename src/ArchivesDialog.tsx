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
    <dialog
      ref={dialog}
      aria-label="Archives"
      className="archives-dialog"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) dismiss();
      }}
    >
      <div className="archives-heading">
        <h2>Archives</h2>
        <button disabled={busy} onClick={dismiss}>
          Close
        </button>
      </div>
      <p className="muted">
        Sessions and removed tabs are kept for 30 days from archival. Expired archives are purged at
        startup and when archiving. Working files are never deleted.
      </p>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {data?.cleanupError && (
        <p className="error" role="alert">
          Archive cleanup needs attention: {data.cleanupError}
        </p>
      )}
      {!data && !error && <p role="status">Loading archives…</p>}
      {data?.entries.length === 0 && <p>No archives.</p>}
      {!!data?.entries.length && (
        <table className="archives-table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Type / context</th>
              <th>Archived</th>
              <th>Expires</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {data.entries.map((entry) => (
              <tr key={entry.id}>
                <td>
                  {entry.name}
                  <small>Archive #{entry.id}</small>
                </td>
                <td>
                  {entry.kind === 'session' ? 'Session' : 'Tab'}
                  <small>
                    {entry.projectName}
                    {entry.kind === 'tab' ? ` / ${entry.sessionName}` : ''}
                  </small>
                </td>
                <td>{new Date(entry.archivedAt * 1000).toLocaleDateString()}</td>
                <td>
                  {entry.purging
                    ? 'Deletion pending'
                    : new Date(entry.expiresAt * 1000).toLocaleDateString()}
                </td>
                <td>
                  <div className="archive-actions">
                    <button
                      disabled={busy || entry.purging || entry.expiresAt * 1000 <= Date.now()}
                      onClick={async () => {
                        if (
                          entry.restartTerminals > 0 &&
                          !(await confirm(
                            `Restore “${entry.name}” and launch ${entry.restartTerminals} previously running terminal command(s) in new processes?`,
                          ))
                        )
                          return;
                        await act('restore_archive', { id: entry.id });
                      }}
                    >
                      Restore
                    </button>
                    <button
                      disabled={busy}
                      onClick={async () => {
                        if (
                          await confirm(
                            `Permanently delete archive “${entry.name}” and its DevCenter history? This cannot be undone. Working files stay untouched.`,
                          )
                        )
                          await act('purge_archive', { id: entry.id, confirmed: true });
                      }}
                    >
                      Delete permanently
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="dialog-actions">
        <button
          disabled={busy}
          onClick={async () => {
            if (
              await confirm(
                'Permanently purge archives older than 30 days and retry pending cleanup?',
              )
            )
              await act('prune_archives', { confirmed: true });
          }}
        >
          Purge expired
        </button>
      </div>
    </dialog>
  );
}
