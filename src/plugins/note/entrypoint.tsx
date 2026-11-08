import { t } from '../../i18n';
import { Button, Input, TabPanel } from '../../components/Controls';
import { useEffect, useState } from 'react';
import { useSdk, useTabInteractions } from '../../sdk';
import type { PluginContentProps } from '../types';
import { NoteModel } from './model';
import { MarkdownEditor } from './MarkdownEditor';
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
      void note.save().catch(() => {});
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
      note.scheduleSave();
    } catch (error) {
      setInputError(String(error));
    }
  }
  return (
    <TabPanel
      className="plugin-panel note-panel"
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!visible}
    >
      <div className="note-workspace">
        <aside className="note-sidebar" aria-label={t('Notes')}>
          <div className="note-sidebar-heading">
            <strong>{t('Notes')}</strong>
            <Button
              disabled={note.loading || !note.notes.length}
              onClick={() =>
                update(() => {
                  note.add();
                })
              }
            >
              {t('Add note')}
            </Button>
          </div>
          <div className="note-list">
            {note.notes.map((item) => (
              <Button
                key={item.id}
                aria-current={item.id === note.selectedNoteId ? 'true' : undefined}
                title={item.name}
                onClick={() => update(() => note.select(item.id))}
              >
                {item.name}
              </Button>
            ))}
          </div>
        </aside>
        <div className="note-editor">
          <div className="note-toolbar">
            <span>{t('Markdown · saved automatically')}</span>
            <Button
              disabled={note.loading || note.saving}
              onClick={() => void note.load(true).catch(() => {})}
            >
              {note.dirty ? t('Discard edits and reload') : t('Reload')}
            </Button>
          </div>
          {!!note.notes.length && (
            <NoteName
              key={note.selectedNoteId}
              name={note.getNote().name}
              disabled={note.loading}
              rename={(name) => update(() => note.rename(name))}
            />
          )}
          <MarkdownEditor
            key={note.selectedNoteId}
            text={note.text}
            disabled={note.loading || !note.notes.length}
            visible={visible}
            onChange={(text) => update(() => note.edit(text))}
          />
        </div>
      </div>
      {(note.error || inputError) && <p role="alert">{inputError || note.error}</p>}
      <div className="plugin-statusbar" role="status">
        {note.loading
          ? t('Loading…')
          : note.saving
            ? t('Saving…')
            : note.dirty
              ? t('Unsaved changes')
              : t('Saved locally')}
        {' · '}
        {new TextEncoder().encode(note.text).length}
        {t(' / 24576 bytes')}
      </div>
    </TabPanel>
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
      {t('Name')}
      <Input
        aria-label={t('Note name')}
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
  return <p>{t('Markdown notes saved locally with this tab. Archiving retains its contents.')}</p>;
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
