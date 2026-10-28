import { useEffect, useRef, useState } from 'react';
import { useSdk, SdkError, type TextDocument } from '../../sdk';
export type { TextDocument } from '../../sdk';
import { Compartment, EditorState } from '@codemirror/state';
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  drawSelection,
} from '@codemirror/view';
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
  undo,
  redo,
} from '@codemirror/commands';
import { search, searchKeymap, openSearchPanel } from '@codemirror/search';
import { useEditorWrap } from './editor-settings';
import { fileEditors } from './editor-guards';
import { readDraft, storeDraft, removeDraft } from './editor-drafts';
import { startMerge } from './editor-merge';
import { editorAppearance, fileLanguage } from './editor-syntax';

export function TextEditor({
  id,
  document,
  visible,
  back,
}: {
  id: number;
  document: TextDocument;
  visible: boolean;
  back: () => Promise<void>;
}) {
  const sdk = useSdk();
  const { readText, writeText } = sdk.desktop;
  const wrapCompartment = useRef(new Compartment());
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView>(null);
  const baseline = useRef(document);
  const saving = useRef(false);
  const persistDraft = useRef(true);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [draftError, setDraftError] = useState('');
  const [notice, setNotice] = useState('');
  const [position, setPosition] = useState({ line: 1, column: 1 });
  const [wrap, setWrap] = useEditorWrap();
  const menu = useRef<HTMLDivElement>(null);
  const menuTrigger = useRef<HTMLButtonElement>(null);
  const menuId = `editor-actions-${id}`;
  const [confirmation, setConfirmation] = useState<'back' | 'reload' | 'overwrite'>();
  const overwriteRevision = useRef<string>(undefined);
  const mergeJob = useRef<ReturnType<typeof startMerge>>(undefined);
  const confirmationRef = useRef(confirmation);
  confirmationRef.current = confirmation;
  const dialog = useRef<HTMLDialogElement>(null);
  const saveRef = useRef<() => void>(() => {});
  const statusRef = useRef({ dirty: false, busy: false });
  function publish(nextDirty: boolean, nextBusy: boolean) {
    if (!view.current) return;
    if (persistDraft.current) {
      try {
        storeDraft(id, baseline.current, view.current.state.doc.toString());
        setDraftError('');
      } catch (error) {
        setDraftError(
          `Could not store the local draft: ${String(error)}. Keep this editor open or save the file.`,
        );
      }
    }
    statusRef.current = { dirty: nextDirty, busy: nextBusy };
    fileEditors.set(id, { path: baseline.current.path, dirty: nextDirty, busy: nextBusy });
    setDirty(nextDirty);
    setBusy(nextBusy);
  }
  async function save(revision?: string) {
    if (!view.current || saving.current || confirmationRef.current) return;
    saving.current = true;
    const text = view.current.state.doc.toString();
    publish(dirty, true);
    setError('');
    try {
      const saved = await sdk.fs.saveText({
        path: baseline.current.path,
        text,
        revision: revision ?? baseline.current.revision,
      });
      if (!view.current) return;
      baseline.current = saved;
      persistDraft.current = true;
      setNotice('');
      publish(view.current.state.doc.toString() !== saved.text, false);
    } catch (error) {
      setError(String(error));
      if (error instanceof SdkError && error.code === 'revision_conflict') {
        try {
          const current = await sdk.fs.readText(baseline.current.path);
          if (view.current && current) {
            overwriteRevision.current = current.revision;
            setConfirmation('overwrite');
          }
        } catch (readError) {
          setError(String(readError));
        }
      }
      publish(statusRef.current.dirty, false);
    } finally {
      saving.current = false;
    }
  }
  saveRef.current = () => {
    void save();
  };
  useEffect(() => {
    if (!host.current) return;
    const clipboardError = (error: unknown) => setError(String(error));
    const languageCompartment = new Compartment();
    const themeCompartment = new Compartment();
    const language = fileLanguage(document.path);
    baseline.current = document;
    let initialText = document.text;
    try {
      const draft = readDraft(id, document.path);
      if (draft && draft.text !== document.text) {
        baseline.current = draft.baseline;
        initialText = draft.text;
        setNotice(
          draft.baseline.revision !== document.revision
            ? 'Local draft restored. The file changed on disk; saving will require confirmation.'
            : 'Local draft restored. Save to apply your changes to the file.',
        );
      }
    } catch (error) {
      persistDraft.current = false;
      setDraftError(`Could not restore the local draft: ${String(error)}`);
    }
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: initialText,
        extensions: [
          languageCompartment.of([]),
          wrapCompartment.current.of(wrap ? EditorView.lineWrapping : []),
          lineNumbers(),
          highlightActiveLine(),
          drawSelection(),
          history(),
          search(),
          EditorState.tabSize.of(2),
          EditorView.contentAttributes.of({
            'aria-label': 'Text file contents',
            spellcheck: 'false',
          }),
          keymap.of([
            {
              key: 'Mod-s',
              run: () => {
                saveRef.current();
                return true;
              },
            },
            {
              key: 'Mod-c',
              run: (editor) => {
                const text = editor.state.sliceDoc(
                  editor.state.selection.main.from,
                  editor.state.selection.main.to,
                );
                void writeText(text).catch(clipboardError);
                return true;
              },
            },
            {
              key: 'Mod-x',
              run: (editor) => {
                const selection = editor.state.selection.main;
                if (selection.empty) return true;
                const text = editor.state.sliceDoc(selection.from, selection.to);
                const beforeCut = editor.state;
                void writeText(text)
                  .then(() => {
                    if (view.current === editor && editor.state === beforeCut)
                      editor.dispatch({
                        changes: { from: selection.from, to: selection.to },
                        userEvent: 'delete.cut',
                      });
                  })
                  .catch(clipboardError);
                return true;
              },
            },
            {
              key: 'Mod-v',
              run: (editor) => {
                void readText()
                  .then((text) => {
                    if (view.current !== editor) return;
                    editor.dispatch(editor.state.replaceSelection(text.replace(/\r\n?/g, '\n')));
                  })
                  .catch(clipboardError);
                return true;
              },
            },
            indentWithTab,
            ...searchKeymap,
            ...historyKeymap,
            ...defaultKeymap,
          ]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              persistDraft.current = true;
              publish(update.state.doc.toString() !== baseline.current.text, saving.current);
            }
            if (update.selectionSet || update.docChanged) {
              const head = update.state.selection.main.head;
              const line = update.state.doc.lineAt(head);
              setPosition({ line: line.number, column: head - line.from + 1 });
            }
          }),
          EditorView.theme({
            '&': { height: '100%', backgroundColor: 'var(--editor)', color: 'var(--text)' },
            '.cm-scroller': {
              overflow: 'auto',
              fontFamily: 'ui-monospace, monospace',
              fontSize: '13px',
            },
            '.cm-gutters': {
              backgroundColor: 'var(--surface)',
              color: 'var(--muted)',
              borderColor: 'var(--separator)',
            },
            '.cm-activeLine': { backgroundColor: 'var(--highlight)' },
            '.cm-cursor': { borderLeftColor: 'var(--text)' },
            '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
              backgroundColor: 'var(--selected)',
            },
            '.cm-panels': { backgroundColor: 'var(--surface)', color: 'var(--text)' },
          }),
          themeCompartment.of(editorAppearance(sdk.theme.getSnapshot().application)),
        ],
      }),
    });
    view.current = editor;
    const unsubscribeTheme = sdk.theme.subscribe(() => {
      editor.dispatch({
        effects: themeCompartment.reconfigure(
          editorAppearance(sdk.theme.getSnapshot().application),
        ),
      });
    });
    if (language) {
      void language.load().then(
        (support) => {
          if (view.current === editor)
            editor.dispatch({ effects: languageCompartment.reconfigure(support) });
        },
        () => {
          if (view.current === editor)
            setError(
              `Could not load ${language.name} syntax highlighting. Reopen the file to retry.`,
            );
        },
      );
    }
    publish(initialText !== baseline.current.text, false);
    editor.focus();
    return () => {
      view.current = null;
      mergeJob.current?.cancel();
      unsubscribeTheme();
      editor.destroy();
      fileEditors.delete(id);
    };
  }, [id, document.path]);
  useEffect(() => {
    view.current?.dispatch({
      effects: wrapCompartment.current.reconfigure(wrap ? EditorView.lineWrapping : []),
    });
  }, [wrap]);
  useEffect(() => {
    if (!visible) menu.current?.hidePopover();
  }, [visible]);
  useEffect(() => {
    if (visible) view.current?.requestMeasure();
  }, [visible]);
  useEffect(() => {
    if (confirmation) dialog.current?.showModal();
  }, [confirmation]);
  useEffect(() => {
    if (!visible || dirty || busy || confirmation) return;
    let cancelled = false;
    let pending = false;
    let queued = false;
    let unwatch: (() => void) | undefined;
    async function refresh() {
      const editor = view.current;
      if (pending) {
        queued = true;
        return;
      }
      if (
        cancelled ||
        !editor ||
        saving.current ||
        statusRef.current.dirty ||
        confirmationRef.current
      )
        return;
      pending = true;
      const before = editor.state.doc;
      const previous = baseline.current;
      try {
        const loaded = await sdk.fs.readText(previous.path);
        if (
          cancelled ||
          view.current !== editor ||
          saving.current ||
          confirmationRef.current ||
          baseline.current !== previous ||
          editor.state.doc !== before ||
          statusRef.current.dirty
        )
          return;
        if (!loaded) throw new Error('File is no longer supported text.');
        if (loaded.revision !== previous.revision) {
          baseline.current = loaded;
          if (loaded.text !== before.toString()) {
            const selection = editor.state.selection.main;
            editor.dispatch({
              changes: { from: 0, to: before.length, insert: loaded.text },
              selection: {
                anchor: Math.min(selection.anchor, loaded.text.length),
                head: Math.min(selection.head, loaded.text.length),
              },
            });
          }
          publish(false, false);
        }
        setError('');
      } catch (error) {
        if (!cancelled && view.current === editor && !saving.current && !statusRef.current.dirty)
          setError(String(error));
      } finally {
        pending = false;
        if (queued && !cancelled) {
          queued = false;
          void refresh();
        }
      }
    }
    void sdk.fs
      .watchText(baseline.current.path, () => void refresh())
      .then((stop) => {
        if (cancelled) stop();
        else {
          unwatch = stop;
          void refresh();
        }
      })
      .catch((error) => {
        if (!cancelled) setError(String(error));
      });
    const focus = () => void refresh();
    window.addEventListener('focus', focus);
    return () => {
      cancelled = true;
      unwatch?.();
      window.removeEventListener('focus', focus);
    };
  }, [visible, id, document.path, sdk, dirty, busy, confirmation]);
  async function reload() {
    if (saving.current) return;
    saving.current = true;
    publish(dirty, true);
    setError('');
    const beforeReload = view.current?.state.doc;
    try {
      const loaded = await sdk.fs.readText(baseline.current.path);
      if (!loaded) throw new Error('File is no longer supported text.');
      if (view.current?.state.doc !== beforeReload)
        throw new Error('Content changed while reloading; your edits have been kept.');
      baseline.current = loaded;
      setNotice('');
      view.current?.dispatch({
        changes: { from: 0, to: view.current.state.doc.length, insert: loaded.text },
      });
      publish(false, false);
    } catch (error) {
      setError(String(error));
      publish(statusRef.current.dirty, false);
    } finally {
      saving.current = false;
    }
  }
  async function mergeChanges() {
    const editor = view.current;
    if (!editor || saving.current) return;
    saving.current = true;
    const before = editor.state.doc;
    const original = baseline.current;
    publish(statusRef.current.dirty, true);
    setError('');
    try {
      const disk = await sdk.fs.readText(original.path);
      if (!disk) throw new Error('File is no longer supported text.');
      if (view.current !== editor) return;
      const job = startMerge(original.text, before.toString(), disk.text);
      mergeJob.current = job;
      const merged = await job.result;
      if (view.current !== editor) return;
      if (editor.state.doc !== before || baseline.current !== original)
        throw new Error('Content changed while merging; your edits have been kept. Try again.');
      baseline.current = disk;
      editor.dispatch({ changes: { from: 0, to: before.length, insert: merged.text } });
      publish(merged.text !== disk.text, false);
      setNotice(
        merged.conflict
          ? 'Changes merged into the editor with conflict markers. Resolve them, then save.'
          : 'Changes merged into the editor. Review the result, then save.',
      );
      editor.focus();
    } catch (error) {
      if (view.current === editor) setError(String(error));
    } finally {
      mergeJob.current = undefined;
      saving.current = false;
      if (view.current === editor)
        publish(editor.state.doc.toString() !== baseline.current.text, false);
    }
  }
  async function leave() {
    if (saving.current) return;
    saving.current = true;
    publish(dirty, true);
    let left = false;
    try {
      removeDraft(id, baseline.current.path);
      await back();
      left = true;
    } catch (error) {
      setError(String(error));
    } finally {
      saving.current = false;
      if (!left) publish(dirty, false);
    }
  }
  return (
    <div className="text-editor">
      <div className="plugin-toolbar editor-toolbar">
        <span className="editor-path" title={document.path}>
          {document.path}
          {dirty ? ' •' : ''}
        </span>
        <button ref={menuTrigger} popoverTarget={menuId} aria-haspopup="menu">
          Actions ▾
        </button>
        <div
          id={menuId}
          ref={menu}
          popover="auto"
          role="menu"
          aria-label="Editor actions"
          className="editor-menu"
          onToggle={(event) => {
            if (!event.currentTarget.matches(':popover-open')) return;
            const trigger = menuTrigger.current?.getBoundingClientRect();
            if (trigger) {
              event.currentTarget.style.right = `${Math.max(8, window.innerWidth - trigger.right)}px`;
              event.currentTarget.style.top = `${Math.max(8, Math.min(trigger.bottom + 4, window.innerHeight - event.currentTarget.offsetHeight - 8))}px`;
            }
            event.currentTarget.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
          }}
          onKeyDown={(event) => {
            if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
            event.preventDefault();
            const items = [
              ...event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'),
            ];
            const index = items.indexOf(globalThis.document.activeElement as HTMLButtonElement);
            const next =
              event.key === 'Home'
                ? 0
                : event.key === 'End'
                  ? items.length - 1
                  : (index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
            items[next]?.focus();
          }}
        >
          <button
            role="menuitem"
            disabled={busy}
            onClick={() => {
              menu.current?.hidePopover();
              if (dirty) setConfirmation('back');
              else void leave();
            }}
          >
            ← Files
          </button>
          <button
            role="menuitem"
            disabled={busy || !dirty}
            onClick={() => {
              menu.current?.hidePopover();
              void save();
            }}
          >
            Save <kbd>Ctrl+S</kbd>
          </button>
          <button
            role="menuitem"
            disabled={busy}
            onClick={() => {
              menu.current?.hidePopover();
              if (dirty) setConfirmation('reload');
              else void reload();
            }}
          >
            Reload
          </button>
          <div role="separator" />
          <button
            role="menuitem"
            onClick={() => {
              menu.current?.hidePopover();
              if (view.current) {
                undo(view.current);
                view.current.focus();
              }
            }}
          >
            Undo <kbd>Ctrl+Z</kbd>
          </button>
          <button
            role="menuitem"
            onClick={() => {
              menu.current?.hidePopover();
              if (view.current) {
                redo(view.current);
                view.current.focus();
              }
            }}
          >
            Redo <kbd>Ctrl+Shift+Z</kbd>
          </button>
          <button
            role="menuitem"
            onClick={() => {
              menu.current?.hidePopover();
              if (view.current) openSearchPanel(view.current);
            }}
          >
            Find / replace <kbd>Ctrl+F</kbd>
          </button>
          <div role="separator" />
          <div className="editor-menu-heading">Settings</div>
          <button
            role="menuitemcheckbox"
            aria-checked={wrap}
            onClick={() => {
              setWrap(!wrap);
              menu.current?.hidePopover();
              view.current?.focus();
            }}
          >
            Wrap long lines <span>{wrap ? 'On' : 'Off'}</span>
          </button>
        </div>
      </div>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {draftError && (
        <p className="error" role="alert">
          {draftError}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <div className="editor-surface" ref={host} />
      <footer className="plugin-statusbar editor-status" aria-label="Editor status">
        Ln {position.line}, Col {position.column} · UTF-8{baseline.current.bom ? ' BOM' : ''} ·{' '}
        {baseline.current.lineEnding} ·{' '}
        <span className={dirty ? 'editor-modified-label' : undefined} role="status">
          {dirty
            ? draftError
              ? 'Unsaved changes · Local draft unavailable'
              : 'Unsaved changes · Draft saved locally'
            : 'Saved'}
        </span>
      </footer>
      {confirmation && (
        <dialog
          ref={dialog}
          aria-label={confirmation === 'overwrite' ? 'File changed on disk' : 'Unsaved changes'}
          onCancel={() => setConfirmation(undefined)}
        >
          <h2>{confirmation === 'overwrite' ? 'File changed on disk' : 'Unsaved changes'}</h2>
          <p>
            {confirmation === 'overwrite'
              ? `${document.path} changed on disk. Cancel to manage the changes yourself, force save to replace the disk version, or merge your edits into the latest disk version for review. Merging does not save the file.`
              : `Discard your changes to ${document.path}?`}
          </p>
          <div className="dialog-actions">
            <button autoFocus onClick={() => setConfirmation(undefined)}>
              Cancel
            </button>
            {confirmation === 'overwrite' && (
              <button
                onClick={() => {
                  setConfirmation(undefined);
                  confirmationRef.current = undefined;
                  void mergeChanges();
                }}
              >
                Merge into editor
              </button>
            )}
            <button
              onClick={() => {
                const action = confirmation;
                setConfirmation(undefined);
                if (action === 'back') void leave();
                else if (action === 'overwrite') {
                  confirmationRef.current = undefined;
                  void save(overwriteRevision.current);
                } else void reload();
              }}
            >
              {confirmation === 'overwrite' ? 'Force save' : 'Discard changes'}
            </button>
          </div>
        </dialog>
      )}
    </div>
  );
}
