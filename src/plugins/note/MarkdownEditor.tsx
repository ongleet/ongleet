import { t, subscribeLocale } from '../../i18n';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Annotation, Compartment, EditorState, Transaction } from '@codemirror/state';
import { EditorView, drawSelection, keymap, placeholder } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { useSdk } from '../../sdk';
import { editorAppearance, fileLanguage } from '../files/editor-syntax';

const externalChange = Annotation.define<boolean>();

export function MarkdownEditor({
  text,
  disabled,
  visible,
  onChange,
}: {
  text: string;
  disabled: boolean;
  visible: boolean;
  onChange: (text: string) => void;
}) {
  const sdk = useSdk();
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView>(null);
  const props = useRef({ text, onChange });
  props.current = { text, onChange };
  const editable = useRef(new Compartment());
  const [, render] = useState(0);

  useEffect(() => {
    if (!host.current) return;
    const language = new Compartment();
    const theme = new Compartment();
    const localization = new Compartment();
    const localeExtensions = () => [
      placeholder(t('# Session notes\n\n- [ ] Next task')),
      EditorView.contentAttributes.of({ 'aria-label': t('Markdown note'), spellcheck: 'false' }),
    ];
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: props.current.text,
        extensions: [
          language.of([]),
          theme.of(editorAppearance(sdk.theme.getSnapshot().application)),
          editable.current.of(EditorState.readOnly.of(disabled)),
          EditorView.lineWrapping,
          drawSelection(),
          history(),
          keymap.of([...historyKeymap, ...defaultKeymap]),
          EditorState.tabSize.of(2),
          localization.of(localeExtensions()),
          EditorView.updateListener.of((update) => {
            if (
              update.docChanged &&
              !update.transactions.some((transaction) => transaction.annotation(externalChange))
            ) {
              props.current.onChange(update.state.doc.toString());
              render((value) => value + 1);
            }
          }),
          EditorView.theme({
            '&': { height: '100%', color: 'var(--text)', backgroundColor: 'var(--editor)' },
            '.cm-scroller': { overflow: 'auto', fontFamily: 'monospace', lineHeight: '1.5' },
            '.cm-content': { padding: '12px' },
            '.cm-cursor': { borderLeftColor: 'var(--text)' },
            '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': {
              backgroundColor: 'var(--selected)',
            },
          }),
        ],
      }),
    });
    view.current = editor;
    const unsubscribeLocale = subscribeLocale(() =>
      editor.dispatch({ effects: localization.reconfigure(localeExtensions()) }),
    );
    const unsubscribe = sdk.theme.subscribe(() => {
      editor.dispatch({
        effects: theme.reconfigure(editorAppearance(sdk.theme.getSnapshot().application)),
      });
    });
    void fileLanguage('note.md')
      ?.load()
      .then(
        (support) => {
          if (view.current === editor) editor.dispatch({ effects: language.reconfigure(support) });
        },
        () => {
          // Keep the editor usable if the optional language chunk cannot load.
        },
      );
    return () => {
      view.current = null;
      unsubscribe();
      unsubscribeLocale();
      editor.destroy();
    };
  }, [sdk]);

  useLayoutEffect(() => {
    const editor = view.current;
    if (editor && editor.state.doc.toString() !== text)
      editor.dispatch({
        changes: { from: 0, to: editor.state.doc.length, insert: text },
        annotations: [externalChange.of(true), Transaction.addToHistory.of(false)],
      });
  });
  useEffect(() => {
    view.current?.dispatch({
      effects: editable.current.reconfigure(EditorState.readOnly.of(disabled)),
    });
  }, [disabled]);
  useEffect(() => {
    if (visible) view.current?.requestMeasure();
  }, [visible]);
  return <div className="note-markdown-editor" ref={host} />;
}
