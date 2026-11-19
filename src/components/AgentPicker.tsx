import { t } from '../i18n';
import { Select } from './Controls';
import type { AgentTarget } from './agent-recipient';

/** Shared compact recipient control for selections and discussion messages. */
export function AgentPicker({
  targets,
  target,
  onChange,
  disabled = false,
  loading = false,
  onFocus,
}: {
  targets: AgentTarget[];
  target: number | undefined;
  onChange: (target: number | undefined) => void;
  disabled?: boolean;
  loading?: boolean;
  onFocus?: () => void;
}) {
  return (
    <label className="agent-picker">
      {t('Agent')}
      <Select
        aria-label={t('Agent')}
        onFocus={onFocus}
        value={target ?? ''}
        disabled={disabled || loading || !targets.length}
        onChange={(event) => onChange(event.target.value ? Number(event.target.value) : undefined)}
      >
        <option value="" disabled>
          {loading
            ? t('Finding agents…')
            : targets.length
              ? t('Choose an agent')
              : t('No agent configured in this session')}
        </option>
        {targets.map((agent) => (
          <option key={agent.id} value={agent.id}>
            {agent.name}
          </option>
        ))}
      </Select>
    </label>
  );
}
