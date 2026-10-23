import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';
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
import { fileEditors } from './editor-guards';

export type TextDocument = {
  path: string;
  text: string;
  revision: string;
  lineEnding: string;
  bom: boolean;
};
export function TextEditor({
  id,
  document,
  visible,
  back,
}: {
  id: number;
  document: TextDocument;
  visible: boolean;
  back: () => void;
}) {
  const wrapCompartment = useRef(new Compartment());
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView>(null);
  const baseline = useRef(document);
  const saving = useRef(false);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [position, setPosition] = useState({ line: 1, column: 1 });
  const [wrap, setWrap] = useState(false);
  const [confirmation, setConfirmation] = useState<'back' | 'reload'>();
  const dialog = useRef<HTMLDialogElement>(null);
  const saveRef = useRef<() => void>(() => {});
  const statusRef = useRef({ dirty: false, busy: false });
  function publish(nextDirty: boolean, nextBusy: boolean) {
    if (!view.current) return;
    statusRef.current = { dirty: nextDirty, busy: nextBusy };
    fileEditors.set(id, { path: baseline.current.path, dirty: nextDirty, busy: nextBusy });
    setDirty(nextDirty);
    setBusy(nextBusy);
  }
  async function save() {
    if (!view.current || saving.current) return;
    saving.current = true;
    const text = view.current.state.doc.toString();
    publish(dirty, true);
    setError('');
    try {
      const saved = await invoke<TextDocument>('save_plugin_text', {
        id,
        path: baseline.current.path,
        text,
        revision: baseline.current.revision,
      });
      if (!view.current) return;
      baseline.current = saved;
      publish(view.current.state.doc.toString() !== saved.text, false);
    } catch (error) {
      setError(String(error));
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
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: document.text,
        extensions: [
          wrapCompartment.current.of([]),
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
            if (update.docChanged)
              publish(update.state.doc.toString() !== baseline.current.text, saving.current);
            if (update.selectionSet || update.docChanged) {
              const head = update.state.selection.main.head;
              const line = update.state.doc.lineAt(head);
              setPosition({ line: line.number, column: head - line.from + 1 });
            }
          }),
          EditorView.theme(
            {
              '&': { height: '100%', backgroundColor: '#121923', color: '#dce3ec' },
              '.cm-scroller': {
                overflow: 'auto',
                fontFamily: 'ui-monospace, monospace',
                fontSize: '13px',
              },
              '.cm-gutters': {
                backgroundColor: '#191f29',
                color: '#8798af',
                borderColor: '#303a48',
              },
              '.cm-activeLine': { backgroundColor: '#ffffff06' },
              '.cm-cursor': { borderLeftColor: '#dce3ec' },
              '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
                backgroundColor: '#39516b',
              },
              '.cm-panels': { backgroundColor: '#191f29', color: '#dce3ec' },
            },
            { dark: true },
          ),
        ],
      }),
    });
    view.current = editor;
    publish(false, false);
    editor.focus();
    return () => {
      view.current = null;
      editor.destroy();
      fileEditors.delete(id);
    };
  }, [id, document.path]);
  useEffect(() => {
    if (visible) view.current?.requestMeasure();
  }, [visible]);
  useEffect(() => {
    if (confirmation) dialog.current?.showModal();
  }, [confirmation]);
  async function reload() {
    if (saving.current) return;
    saving.current = true;
    publish(dirty, true);
    setError('');
    const beforeReload = view.current?.state.doc;
    try {
      const loaded = await invoke<TextDocument | null>('read_plugin_text', {
        id,
        path: baseline.current.path,
      });
      if (!loaded) throw new Error('File is no longer supported text.');
      if (view.current?.state.doc !== beforeReload)
        throw new Error('Content changed while reloading; your edits have been kept.');
      baseline.current = loaded;
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
  return (
    <div className="text-editor">
      <div className="plugin-toolbar editor-toolbar">
        <button disabled={busy} onClick={() => (dirty ? setConfirmation('back') : back())}>
          ← Files
        </button>
        <span className="editor-path" title={document.path}>
          {document.path}
          {dirty ? ' •' : ''}
        </span>
        <button disabled={busy || !dirty} onClick={() => void save()}>
          {busy ? 'Working…' : 'Save'}
        </button>
        <button disabled={busy} onClick={() => (dirty ? setConfirmation('reload') : void reload())}>
          Reload
        </button>
        <button
          title="Undo (Ctrl+Z)"
          onClick={() => {
            if (view.current) {
              undo(view.current);
              view.current.focus();
            }
          }}
        >
          Undo
        </button>
        <button
          title="Redo (Ctrl+Shift+Z)"
          onClick={() => {
            if (view.current) {
              redo(view.current);
              view.current.focus();
            }
          }}
        >
          Redo
        </button>
        <button
          onClick={() => {
            if (view.current) openSearchPanel(view.current);
          }}
        >
          Find / replace
        </button>
        <label className="editor-wrap">
          <input
            type="checkbox"
            checked={wrap}
            onChange={(event) => {
              setWrap(event.target.checked);
              // Reconfigure only the wrapping extension, preserving history and selection.
              view.current?.dispatch({
                effects: wrapCompartment.current.reconfigure(
                  event.target.checked ? EditorView.lineWrapping : [],
                ),
              });
            }}
          />
          Wrap
        </label>
      </div>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="editor-surface" ref={host} />
      <div className="editor-status">
        Ln {position.line}, Col {position.column} · UTF-8{baseline.current.bom ? ' BOM' : ''} ·{' '}
        {baseline.current.lineEnding} · {dirty ? 'Unsaved changes' : 'Saved'}
      </div>
      {confirmation && (
        <dialog
          ref={dialog}
          aria-label="Unsaved changes"
          onCancel={() => setConfirmation(undefined)}
        >
          <h2>Unsaved changes</h2>
          <p>Discard your changes to {document.path}?</p>
          <div className="dialog-actions">
            <button autoFocus onClick={() => setConfirmation(undefined)}>
              Cancel
            </button>
            <button
              onClick={() => {
                const action = confirmation;
                setConfirmation(undefined);
                if (action === 'back') back();
                else void reload();
              }}
            >
              Discard changes
            </button>
          </div>
        </dialog>
      )}
    </div>
  );
}
