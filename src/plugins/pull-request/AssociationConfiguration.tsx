import { useEffect, useState } from 'react';
import { t } from '../../i18n';
import { Button, Input } from '../../components/Controls';
import { useSdk } from '../../sdk';
import { parseAssociation } from './github';

export function AssociationConfiguration() {
  const sdk = useSdk();
  const [revision, setRevision] = useState<number>();
  const [target, setTarget] = useState('');
  const [number, setNumber] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    let active = true;
    void sdk.storage
      .read<{ repository: string; number: number } | null>('session')
      .then((saved) => {
        if (!active) return;
        setRevision(saved?.revision ?? 0);
        setTarget(saved?.value?.repository ?? '');
        setNumber(String(saved?.value?.number ?? ''));
      })
      .catch((error) => {
        if (active) setError(String(error));
      });
    return () => {
      active = false;
    };
  }, [sdk]);
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (busy || revision === undefined) return;
        setBusy(true);
        setError('');
        void (async () => {
          try {
            const association = parseAssociation(target, number ? Number(number) : undefined);
            const saved = await sdk.storage.replace('session', revision, 1, association);
            setRevision(saved.revision);
          } catch (error) {
            setError(String(error));
          } finally {
            setBusy(false);
          }
        })();
      }}
    >
      <fieldset disabled={busy || revision === undefined}>
        <label>
          {t('Repository or PR URL')}
          <Input value={target} onChange={(event) => setTarget(event.target.value)} />
        </label>
        <label>
          {t('PR number')}
          <Input
            type="number"
            min="1"
            value={number}
            onChange={(event) => setNumber(event.target.value)}
          />
        </label>
        <Button>{t('Save')}</Button>
      </fieldset>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </form>
  );
}
