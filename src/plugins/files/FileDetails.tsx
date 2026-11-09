import { t } from '../../i18n';
import { Button } from '../../components/Controls';
import { useState } from 'react';
import { ImageViewer, imageType } from './ImageViewer';
import { useSdk } from '../../sdk';

export type UnsupportedFile = { path: string; reason: string };

// These are editor limitations; missing or inaccessible files remain errors.
export function editorLimitation(error: unknown): string | undefined {
  const message = String(error);
  if (message.includes('Text editor limit is 1 MiB'))
    return 'This file exceeds the editor’s 1 MiB limit.';
  if (message.includes('Mixed or legacy line endings'))
    return 'This file uses line endings the editor does not support.';
}

export function FileDetails({
  file,
  close,
}: {
  file: UnsupportedFile;
  close: () => Promise<void>;
}) {
  const sdk = useSdk();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const name = file.path.split('/').at(-1) ?? file.path;
  const extension = name.includes('.') ? name.slice(name.lastIndexOf('.') + 1) : '';
  async function openExternally() {
    setBusy(true);
    setError('');
    try {
      await sdk.desktop.openFile(file.path);
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="file-details" aria-label={t('File details')}>
      <h2>{name}</h2>
      {imageType(file.path) ? (
        <ImageViewer path={file.path} />
      ) : (
        <p className="muted">{file.reason}</p>
      )}
      <dl>
        <dt>{t('Name')}</dt>
        <dd>{name}</dd>
        <dt>{t('Path')}</dt>
        <dd>{file.path}</dd>
        <dt>{t('Extension')}</dt>
        <dd>{extension ? `.${extension}` : t('None')}</dd>
      </dl>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <div className="file-details-actions">
        <Button disabled={busy} onClick={() => void openExternally()}>
          {busy ? t('Opening…') : t('Open in default application')}
        </Button>
        <Button
          disabled={busy}
          onClick={() => void close().catch((error) => setError(String(error)))}
        >
          {t('Close file')}
        </Button>
      </div>
    </section>
  );
}
