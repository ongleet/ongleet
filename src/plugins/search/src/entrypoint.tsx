import { useEffect, useRef, useState } from 'react';
import { Button, Input, TabPanel } from '../../../components/Controls';
import { Checkbox } from '../../../components/Checkbox';
import { t } from '../../../i18n';
import { useSdk, type FileSearch } from '../../../sdk';
import type { PluginContentProps } from '../../types';
import type { ConfigurationEditorProps } from '../..';
import '../assets/search.css';

function ConfigurationEditor({ value, onChange }: ConfigurationEditorProps) {
  let folder = '';
  try {
    folder = JSON.parse(value).folder ?? '';
  } catch {
    /* Host reports invalid JSON. */
  }
  return (
    <label>
      {t('Initial folder')}
      <Input
        value={folder}
        placeholder={t('Session directory')}
        onChange={(e) => onChange(JSON.stringify({ folder: e.target.value || null }))}
      />
    </label>
  );
}

function Content({ tab, visible }: PluginContentProps) {
  const sdk = useSdk();
  const folder = (tab.config as { folder?: string }).folder ?? '';
  const [query, setQuery] = useState('');
  const [include, setInclude] = useState('');
  const [exclude, setExclude] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [showHidden, setShowHidden] = useState(false);
  const [result, setResult] = useState<FileSearch>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [refresh, setRefresh] = useState(0);
  const [selected, setSelected] = useState<FileSearch['matches'][number]>();
  const queue = useRef(Promise.resolve());
  useEffect(() => {
    let cancelled = false;
    setError('');
    setSelected(undefined);
    setResult(undefined);
    setBusy(!!query && !!folder);
    if (!query || !folder) return;
    const timer = window.setTimeout(() => {
      queue.current = queue.current.then(async () => {
        if (cancelled) return;
        try {
          const found = await sdk.fs.searchFiles({
            folder,
            query,
            showHidden,
            content: true,
            caseSensitive,
            include,
            exclude,
          });
          if (!cancelled) setResult(found);
        } catch (error) {
          if (!cancelled) setError(String(error));
        } finally {
          if (!cancelled) setBusy(false);
        }
      });
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [sdk, folder, query, showHidden, caseSensitive, include, exclude, refresh]);
  return (
    <TabPanel
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
      hidden={!visible}
      className="plugin-panel search-panel"
    >
      <div className="search-toolbar">
        <Input
          aria-label={t('Search file contents')}
          placeholder={t('Search file contents')}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          maxLength={256}
        />
        <label className="checkbox-field">
          <Checkbox checked={caseSensitive} onChange={(e) => setCaseSensitive(e.target.checked)} />
          {t('Match case')}
        </label>
        <label className="checkbox-field">
          <Checkbox checked={showHidden} onChange={(e) => setShowHidden(e.target.checked)} />
          {t('Show hidden files')}
        </label>
        <Button type="button" onClick={() => setRefresh((n) => n + 1)}>
          {t('Refresh')}
        </Button>
      </div>
      <div className="search-toolbar search-filters">
        <label>
          {t('Files to include')}
          <Input
            value={include}
            maxLength={4096}
            placeholder="src/**, *.ts"
            onChange={(e) => setInclude(e.target.value)}
          />
        </label>
        <label>
          {t('Files to exclude')}
          <Input
            value={exclude}
            maxLength={4096}
            placeholder="**/node_modules/**, *.min.js"
            onChange={(e) => setExclude(e.target.value)}
          />
        </label>
      </div>
      <p className="muted">
        {t(
          'Separate patterns with commas. Use * for names, ** for folders, and ? for one character.',
        )}
      </p>
      <p className="muted">{folder || t('Configure a folder to search.')}</p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <p role="status">
        {busy
          ? t('Searching…')
          : result
            ? t('{count} matching lines', { count: result.matches.length })
            : ''}
        {result?.truncated ? t(' · Search limit reached; narrow your search') : ''}
        {result && result.skipped > 0
          ? t(' · {value0} entries could not be searched', { value0: result.skipped })
          : ''}
      </p>
      <div className="search-body" aria-busy={busy}>
        <ul className="file-list search-matches">
          {result?.matches.map((match) => (
            <li key={`${match.path}:${match.line}`}>
              <Button
                type="button"
                title={match.path}
                aria-pressed={selected === match}
                onClick={() => setSelected(match)}
              >
                <strong>
                  {match.relativePath}:{match.line}
                </strong>
                <code>{match.text}</code>
              </Button>
            </li>
          ))}
        </ul>
        {selected && (
          <section className="search-preview">
            <h3>
              {selected.relativePath}:{selected.line}
            </h3>
            <pre>{selected.text}</pre>
            <Button
              type="button"
              onClick={() =>
                void sdk.desktop.openFile(selected.path).catch((e) => setError(String(e)))
              }
            >
              {t('Open file')}
            </Button>
          </section>
        )}
      </div>
    </TabPanel>
  );
}
export default { Content, ConfigurationEditor, toDraft: JSON.stringify, fromDraft: JSON.parse };
