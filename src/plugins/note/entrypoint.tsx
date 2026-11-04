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
          replace: (revision, value) => sdk.storage.replace('tab', revision, 1, value),
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
    state: () => ({
      summary: note.loading ? 'Loading Note' : note.error ? 'Note save failed' : 'Markdown note',
      data: {
        text: note.text,
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
        note.replace(args.text as string, args.revision as number, signal),
    },
  });
  return (
    <section
      className="plugin-panel note-panel"
      role="tabpanel"
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!visible}
    >
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
      <textarea
        aria-label="Markdown note"
        spellCheck={false}
        disabled={note.loading}
        value={note.text}
        placeholder={'# Session notes\n\n- [ ] Next task'}
        onChange={(event) => {
          try {
            note.edit(event.target.value);
            setInputError('');
            void note.save().catch(() => {});
          } catch (error) {
            setInputError(String(error));
          }
        }}
      />
      {(note.error || inputError) && <p role="alert">{inputError || note.error}</p>}
      <div className="plugin-status-bar" role="status">
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

function ConfigurationEditor() {
  return <p>A Markdown note saved locally with this tab. Archiving retains its contents.</p>;
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
