import { useState } from 'react';
import { t } from '../../i18n';
import { Checkbox } from '../../components/Checkbox';
import { Button, Select } from '../../components/Controls';
import { useSdk, useSessionSettings } from '../../sdk';
import { browserDefaults, type BrowserPreferences } from './session-settings';

export function BrowserFields({
  value,
  disabled,
  change,
}: {
  value: BrowserPreferences;
  disabled?: boolean;
  change: (patch: Partial<BrowserPreferences>) => void;
}) {
  return (
    <>
      <label className="checkbox-field">
        <Checkbox
          checked={value.showHidden}
          disabled={disabled}
          onChange={(event) => change({ showHidden: event.target.checked })}
        />
        {t('Show hidden files')}
      </label>
      <label>
        {t('Sort order (folders first)')}
        <Select
          value={value.order}
          disabled={disabled}
          onChange={(event) => change({ order: event.target.value as BrowserPreferences['order'] })}
        >
          <option value="nameAscending">{t('Name A–Z')}</option>
          <option value="nameDescending">{t('Name Z–A')}</option>
          <option value="dateDescending">{t('Newest')}</option>
          <option value="dateAscending">{t('Oldest')}</option>
          <option value="sizeAscending">{t('Smallest')}</option>
          <option value="sizeDescending">{t('Largest')}</option>
        </Select>
      </label>
    </>
  );
}

export function SessionConfiguration({ scope }: { scope: 'settings' | 'globalSettings' }) {
  const shared = useSessionSettings(useSdk(), browserDefaults);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  return (
    <>
      <BrowserFields
        value={scope === 'globalSettings' ? shared.globalValue : shared.value}
        disabled={shared.loading || busy || !!shared.error}
        change={(patch) => {
          setBusy(true);
          setError('');
          void shared
            .patch(patch, scope)
            .catch((error) => setError(String(error)))
            .finally(() => setBusy(false));
        }}
      />
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
      {(error || shared.error) && (
        <p className="error" role="alert">
          {error || shared.error}
        </p>
      )}
    </>
  );
}
