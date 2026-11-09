import { useState } from 'react';
import { t } from '../../i18n';
import { Button, Input, Select } from '../../components/Controls';
import { useSdk, useSessionSettings } from '../../sdk';
import { githubAuth, sessionAuth } from './auth';
import { DEFAULT_REFRESH_SECONDS, refreshSeconds } from './settings';
import { AssociationConfiguration } from './AssociationConfiguration';
import { GitHubConnection } from './Connection';

export const prSessionDefaults = {
  connection: 'default' as 'default' | 'session',
  refreshIntervalSeconds: DEFAULT_REFRESH_SECONDS,
};

export function SessionConfiguration({
  sessionId,
  scope,
}: {
  sessionId: number;
  scope: 'settings' | 'globalSettings';
}) {
  const shared = useSessionSettings(useSdk(), prSessionDefaults);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const connection = scope === 'globalSettings' ? 'default' : shared.value.connection;
  const value = scope === 'globalSettings' ? shared.globalValue : shared.value;
  return (
    <>
      {scope === 'settings' && <AssociationConfiguration />}
      {scope === 'settings' && (
        <label>
          {t('GitHub account')}
          <Select
            value={shared.value.connection}
            disabled={shared.loading || busy || !!shared.error}
            onChange={(event) => {
              setBusy(true);
              setError('');
              void shared
                .patch({ connection: event.target.value as 'default' | 'session' }, scope)
                .catch((error) => setError(String(error)))
                .finally(() => setBusy(false));
            }}
          >
            <option value="default">{t('Global default account')}</option>
            <option value="session">{t('Account for this session')}</option>
          </Select>
        </label>
      )}
      <label>
        {t('Refresh interval (seconds)')}
        <Input
          key={value.refreshIntervalSeconds}
          type="number"
          min="30"
          max="86400"
          step="1"
          defaultValue={value.refreshIntervalSeconds}
          disabled={shared.loading || busy}
          onBlur={(event) => {
            try {
              const seconds = refreshSeconds({
                refreshIntervalSeconds: Number(event.target.value),
              });
              if (seconds === value.refreshIntervalSeconds) return;
              setBusy(true);
              setError('');
              void shared
                .patch({ refreshIntervalSeconds: seconds }, scope)
                .catch((error) => setError(String(error)))
                .finally(() => setBusy(false));
            } catch (error) {
              setError(String(error));
            }
          }}
        />
      </label>
      <Button
        type="button"
        disabled={shared.loading || busy}
        onClick={() => {
          setBusy(true);
          void shared
            .patch({}, scope, true)
            .catch((error) => setError(String(error)))
            .finally(() => setBusy(false));
        }}
      >
        {scope === 'settings' ? t('Use global defaults') : t('Reset defaults')}
      </Button>
      <p className="muted">
        {connection === 'default'
          ? t('Changing this account affects every session using the global default.')
          : t(
              'This account is shared only by GitHub PR tabs in this session. It ignores launch environment tokens.',
            )}
      </p>
      {(error || shared.error) && (
        <p className="error" role="alert">
          {error || shared.error}
        </p>
      )}
      {!shared.loading && !shared.error && (
        <GitHubConnection
          key={connection}
          controller={connection === 'session' ? sessionAuth(sessionId) : githubAuth}
        />
      )}
    </>
  );
}
