import { useEffect, useState } from 'react';
import { t } from '../../i18n';
import { Button, Input } from '../../components/Controls';
import { useSdk } from '../../sdk';
import type { AuthController } from './auth';

export function GitHubConnection({ controller }: { controller: AuthController }) {
  const sdk = useSdk();
  const [auth, setAuth] = useState(controller.snapshot);
  const [token, setToken] = useState('');
  const [replacing, setReplacing] = useState(false);
  const [error, setError] = useState('');
  useEffect(
    () => controller.subscribe(sdk, () => setAuth(controller.snapshot())),
    [sdk, controller],
  );
  useEffect(() => {
    setToken('');
    setReplacing(false);
    setError('');
  }, [controller, auth.version]);
  return (
    <section aria-label={t('GitHub connection')}>
      <h3>{t('GitHub connection')}</h3>
      {auth.login && (
        <p>
          {t('Connected as ')}
          {auth.login}
        </p>
      )}
      {auth.environment ? (
        <>
          <p className="muted">
            {t('Using ')}
            {auth.environment}
            {t(
              ' from Ongleet’s launch environment. Update it and restart Ongleet to change the token.',
            )}
          </p>
          {(!auth.login || auth.paused) && (
            <Button
              disabled={auth.busy || auth.loading}
              onClick={() => void controller.reconnect(sdk)}
            >
              {t('Reconnect')}
            </Button>
          )}
        </>
      ) : auth.login && !replacing && !auth.paused ? (
        <Button onClick={() => setReplacing(true)} disabled={auth.busy}>
          {t('Replace token')}
        </Button>
      ) : (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            const value = token;
            setToken('');
            void controller.connect(sdk, value).catch((error) => setError(String(error)));
          }}
        >
          <label>
            {t('GitHub personal access token')}
            <Input
              type="password"
              autoComplete="off"
              value={token}
              onChange={(event) => setToken(event.target.value)}
              disabled={auth.busy || auth.loading}
            />
          </label>
          <Button disabled={auth.busy || auth.loading || !token.trim()}>
            {auth.login ? t('Replace token') : t('Connect')}
          </Button>
        </form>
      )}
      {auth.login && (
        <Button
          disabled={auth.busy || auth.loading}
          onClick={() => void controller.disconnect(sdk)}
        >
          {t('Disconnect')}
        </Button>
      )}
      <Button
        onClick={() =>
          void sdk.desktop
            .openUrl('https://github.com/settings/tokens')
            .catch((error) => setError(String(error)))
        }
      >
        {t('GitHub token settings')}
      </Button>
      {auth.loading && <p>{t('Loading GitHub connection…')}</p>}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {auth.error && (
        <p role="alert" className="error">
          {auth.error}
        </p>
      )}
    </section>
  );
}
