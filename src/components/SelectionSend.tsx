import { Button, Dialog, Select, Textarea } from './Controls';
import { createPortal } from 'react-dom';
import { useEffect, useRef, useState } from 'react';
import { useSdk } from '../sdk';
import { availableSendTargets, selectionMessage, sendToTab, type SelectedCode } from './tab-send';
import './selection-send.css';
export function SelectionSend({
  sessionId,
  tabId,
  capture,
  visible,
}: {
  sessionId: number;
  tabId: number;
  capture: () => SelectedCode;
  visible: boolean;
}) {
  const sdk = useSdk();
  const [selection, setSelection] = useState<SelectedCode>();
  const [question, setQuestion] = useState('');
  const [targets, setTargets] = useState<{ id: number; name: string }[]>([]);
  const [target, setTarget] = useState<number>();
  const [busy, setBusy] = useState(false);
  const [loadingTargets, setLoadingTargets] = useState(false);
  const writing = useRef(false);
  const [status, setStatus] = useState('');
  const [error, setError] = useState('');
  const dialog = useRef<HTMLDialogElement>(null);
  const mounted = useRef(true);
  const attempt = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      attempt.current++;
    };
  }, []);
  useEffect(() => {
    if (selection && visible) dialog.current?.showModal();
  }, [selection, visible]);
  useEffect(() => {
    if (!visible) {
      dialog.current?.close();
      setSelection(undefined);
      attempt.current++;
    }
  }, [visible]);
  async function open() {
    setError('');
    setStatus('');
    const request = ++attempt.current;
    try {
      const snapshot = capture();
      selectionMessage(snapshot, '');
      setQuestion('');
      setSelection(snapshot);
      setTargets([]);
      setTarget(undefined);
      setLoadingTargets(true);
      const recipients = await availableSendTargets(sdk.tabs, sessionId, tabId);
      if (!mounted.current || request !== attempt.current) return;
      setLoadingTargets(false);
      setTargets(recipients);
      setTarget(recipients.length === 1 ? recipients[0].id : undefined);
    } catch (error) {
      if (mounted.current && request === attempt.current) {
        setLoadingTargets(false);
        setError(String(error));
      }
    }
  }
  async function send() {
    if (!selection || target === undefined || writing.current) return;
    writing.current = true;
    setBusy(true);
    setError('');
    setStatus('');
    try {
      await sendToTab(sdk.tabs, sessionId, tabId, target, selectionMessage(selection, question));
      if (mounted.current) {
        setStatus('Write acknowledged; agent processing is unknown.');
        setSelection(undefined);
      }
    } catch (error) {
      if (mounted.current)
        setError(`Send failed; delivery may be ambiguous. No retry was made. ${String(error)}`);
    } finally {
      writing.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <div className="selection-send">
      <Button
        type="button"
        disabled={busy}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => void open()}
      >
        Send selection…
      </Button>
      {status && <span role="status">{status}</span>}
      {error && !selection && <span role="alert">{error}</span>}
      {selection &&
        createPortal(
          <Dialog
            ref={dialog}
            aria-label="Send selected code"
            onCancel={(event) => {
              if (busy) event.preventDefault();
              else {
                setSelection(undefined);
                attempt.current++;
              }
            }}
          >
            <form
              onSubmit={(event) => {
                event.preventDefault();
                event.stopPropagation();
                void send();
              }}
            >
              <h2>Send selected code</h2>
              <p>
                {selection.path} · {selection.location}
                {selection.unsaved ? ' · Unsaved buffer' : ''}
              </p>
              <pre className="selection-preview">{selection.text}</pre>
              <label>
                Optional comment or question
                <Textarea
                  autoFocus
                  value={question}
                  onChange={(event) => setQuestion(event.target.value)}
                  disabled={busy}
                />
              </label>
              {targets.length > 1 && (
                <label>
                  Send to
                  <Select
                    value={target ?? ''}
                    disabled={busy}
                    onChange={(event) =>
                      setTarget(event.target.value ? Number(event.target.value) : undefined)
                    }
                  >
                    <option value="">Choose a tab</option>
                    {targets.map((tab) => (
                      <option key={tab.id} value={tab.id}>
                        {tab.name}
                      </option>
                    ))}
                  </Select>
                </label>
              )}
              {loadingTargets ? (
                <p>Finding receiving tabs…</p>
              ) : (
                !targets.length && <p>No available receiving tabs in this session.</p>
              )}
              {error && <p role="alert">{error}</p>}
              <div className="dialog-actions">
                <Button
                  type="button"
                  disabled={busy}
                  onClick={() => {
                    setSelection(undefined);
                    attempt.current++;
                  }}
                >
                  Cancel
                </Button>
                <Button type="submit" disabled={busy || target === undefined}>
                  {busy ? 'Sending…' : targets.length === 1 ? `Send to ${targets[0].name}` : 'Send'}
                </Button>
              </div>
            </form>
          </Dialog>,
          document.body,
        )}
    </div>
  );
}
