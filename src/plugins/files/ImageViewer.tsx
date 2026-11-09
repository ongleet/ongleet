import { useEffect, useState } from 'react';
import { Button } from '../../components/Controls';
import { useSdk } from '../../sdk';
import { t } from '../../i18n';

const formats: Record<string, string> = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  avif: 'image/avif',
  bmp: 'image/bmp',
  ico: 'image/x-icon',
  svg: 'image/svg+xml',
};
export function imageType(path: string): string | undefined {
  return formats[path.split('.').at(-1)?.toLowerCase() ?? ''];
}

export function ImageViewer({ path }: { path: string }) {
  const sdk = useSdk();
  const [url, setUrl] = useState('');
  const [error, setError] = useState('');
  const [scale, setScale] = useState<number | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let cancelled = false;
    let objectUrl = '';
    setUrl('');
    setError('');
    setSize({ width: 0, height: 0 });
    void sdk.fs
      .readImage(path)
      .then((bytes) => {
        if (cancelled) return;
        objectUrl = URL.createObjectURL(
          new Blob([new Uint8Array(bytes)], { type: imageType(path) }),
        );
        setUrl(objectUrl);
      })
      .catch((error) => {
        if (!cancelled) setError(String(error));
      });
    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [sdk, path, revision]);
  return (
    <section className="image-viewer" aria-label={t('Image preview')}>
      <div className="image-toolbar">
        <Button onClick={() => setScale(null)} aria-pressed={scale === null}>
          {t('Fit image')}
        </Button>
        <Button onClick={() => setScale(1)} aria-pressed={scale === 1}>
          {t('Actual size')}
        </Button>
        <Button
          aria-label={t('Zoom out')}
          disabled={!size.width}
          onClick={() => setScale(Math.max(0.1, (scale ?? 1) / 1.25))}
        >
          −
        </Button>
        <Button
          aria-label={t('Zoom in')}
          disabled={!size.width}
          onClick={() => setScale(Math.min(8, (scale ?? 1) * 1.25))}
        >
          +
        </Button>
        <Button onClick={() => setRevision((value) => value + 1)}>{t('Reload image')}</Button>
        {!!size.width && (
          <span className="muted">
            {size.width} × {size.height}
            {scale === null ? '' : ` · ${Math.round(scale * 100)}%`}
          </span>
        )}
      </div>
      {error ? (
        <p className="error" role="alert">
          {error}
        </p>
      ) : !url ? (
        <p role="status">{t('Loading image…')}</p>
      ) : (
        <div className="image-viewport" data-fit={scale === null}>
          <img
            src={url}
            alt={path.split(/[\\/]/).at(-1) ?? path}
            style={
              scale === null ? undefined : { width: size.width ? size.width * scale : undefined }
            }
            onLoad={(event) =>
              setSize({
                width: event.currentTarget.naturalWidth,
                height: event.currentTarget.naturalHeight,
              })
            }
            onError={() => setError(t('This image could not be decoded.'))}
          />
        </div>
      )}
    </section>
  );
}
