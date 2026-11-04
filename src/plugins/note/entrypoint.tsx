import { useEffect, useState } from 'react';
import { useSdk, useTabInteractions } from '../../sdk';
import type { PluginContentProps } from '../types';
import { NoteModel } from './model';
import './note.css';

const models = new Set<NoteModel>();

function Content({ tab, visible }: PluginContentProps) {
  const sdk = useSdk();
  const [, render] = useState(0);
  const [inputError, setInputError] = useState('');
  const [note] = useState(
    () =>
      new NoteModel(
        {
          read: () => sdk.storage.read('tab'),
          replace: (revision, value) => sdk.storage.replace('tab', revision, 2, value),
        },
        () => render((value) => value + 1),
      ),
  );
  useEffect(() => {
    models.add(note);
    void note.load().catch(() => {});
    return () => {
      models.delete(note);
    };
  }, [note]);
  useTabInteractions({
    state: (args) => ({
      summary: note.loading ? 'Loading Note' : note.error ? 'Note save failed' : 'Markdown note',
      data: {
        text:
          note.loading || !note.notes.length
            ? ''
            : note.getNote(args.noteId as string | undefined).text,
        selectedNoteId: note.selectedNoteId,
        notes: note.notes.map(({ id, name }) => ({ id, name })),
        revision: note.revision,
        dirty: note.dirty,
        saving: note.saving,
        loading: note.loading,
        error: note.error || null,
      },
      observedAt: note.observedAt,
      stale: note.loading || note.saving || note.dirty || !!note.error,
    }),
    actions: {
      'set-text': (args, { signal }) =>
        note.replace(
          args.text as string,
          args.revision as number,
          signal,
          args.noteId as string | undefined,
        ),
      'create-note': (args, { signal }) =>
        note.act(args.revision as number, signal, () => note.add(args.name as string)),
      'rename-note': (args, { signal }) =>
        note.act(args.revision as number, signal, () => {
          note.rename(args.name as string, args.noteId as string);
          return args.noteId as string;
        }),
      'select-note': (args, { signal }) =>
        note.act(args.revision as number, signal, () => {
          note.select(args.noteId as string);
          return args.noteId as string;
        }),
    },
  });
  function update(change: () => void) {
    try {
      change();
      setInputError('');
      void note.save().catch(() => {});
    } catch (error) {
      setInputError(String(error));
    }
  }
  return (
    <section
      className="plugin-panel note-panel"
      role="tabpanel"
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!visible}
    >
      <div className="note-workspace">
        <aside className="note-sidebar" aria-label="Notes">
          <div className="note-sidebar-heading">
            <strong>Notes</strong>
            <button
              disabled={note.loading || !note.notes.length}
              onClick={() =>
                update(() => {
                  note.add();
                })
              }
            >
              Add note
            </button>
          </div>
          <div className="note-list">
            {note.notes.map((item) => (
              <button
                key={item.id}
                aria-current={item.id === note.selectedNoteId ? 'true' : undefined}
                title={item.name}
                onClick={() => update(() => note.select(item.id))}
              >
                {item.name}
              </button>
            ))}
          </div>
        </aside>
        <div className="note-editor">
          <div className="note-toolbar">
            <span>Markdown · saved automatically</span>
            <button
              disabled={note.loading || note.saving || !note.dirty}
              onClick={() => void note.save().catch(() => {})}
            >
              Retry save
            </button>
            <button
              disabled={note.loading || note.saving}
              onClick={() => void note.load(true).catch(() => {})}
            >
              {note.dirty ? 'Discard edits and reload' : 'Reload'}
            </button>
          </div>
          {!!note.notes.length && (
            <NoteName
              key={note.selectedNoteId}
              name={note.getNote().name}
              disabled={note.loading}
              rename={(name) => update(() => note.rename(name))}
            />
          )}
          <textarea
            aria-label="Markdown note"
            spellCheck={false}
            disabled={note.loading || !note.notes.length}
            value={note.text}
            placeholder={'# Session notes\n\n- [ ] Next task'}
            onChange={(event) => update(() => note.edit(event.target.value))}
          />
        </div>
      </div>
      {(note.error || inputError) && <p role="alert">{inputError || note.error}</p>}
      <div className="plugin-statusbar" role="status">
        {note.loading
          ? 'Loading…'
          : note.saving
            ? 'Saving…'
            : note.dirty
              ? 'Unsaved changes'
              : 'Saved locally'}
        {' · '}
        {new TextEncoder().encode(note.text).length} / 24576 bytes
      </div>
    </section>
  );
}

function NoteName({
  name,
  disabled,
  rename,
}: {
  name: string;
  disabled: boolean;
  rename: (name: string) => void;
}) {
  const [draft, setDraft] = useState(name);
  useEffect(() => setDraft(name), [name]);
  return (
    <label className="note-name">
      Name
      <input
        aria-label="Note name"
        value={draft}
        disabled={disabled}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => {
          rename(draft.trim());
          setDraft(name);
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') event.currentTarget.blur();
        }}
      />
    </label>
  );
}

function ConfigurationEditor() {
  return <p>Markdown notes saved locally with this tab. Archiving retains its contents.</p>;
}

export default {
  Content,
  ConfigurationEditor,
  toDraft: JSON.stringify,
  fromDraft: JSON.parse,
  beforeQuit: async () => {
    await Promise.all([...models].map((model) => model.save()));
  },
};
