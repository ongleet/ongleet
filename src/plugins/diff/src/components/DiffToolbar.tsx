import { Button } from '../../../../components/Controls';
import { SelectionSend } from '../../../../components/SelectionSend';
import { t } from '../../../../i18n';
import { ComparisonFields } from './ConfigurationEditor';
import { captureDiffSelection } from '../selection';
import type { DiffController } from '../hooks/useDiffController';

/** Configure comparisons and send selected code using the current rendered snapshot. */
export function DiffToolbar({
  controller,
  sessionId,
  tabId,
  visible,
}: {
  controller: DiffController;
  sessionId: number;
  tabId: number;
  visible: boolean;
}) {
  const {
    config,
    setConfig,
    references,
    commits,
    commitError,
    refresh,
    viewer,
    comparison,
    loading,
  } = controller;
  return (
    <form
      className="diff-toolbar"
      onSubmit={(e) => {
        e.preventDefault();
        refresh();
      }}
    >
      <ComparisonFields
        config={config}
        onChange={setConfig}
        references={references}
        commits={commits}
        commitError={commitError}
      />
      <SelectionSend
        sessionId={sessionId}
        tabId={tabId}
        visible={visible}
        capture={() => {
          if (!viewer.current || !comparison) throw new Error(t('Comparison unavailable.'));
          return captureDiffSelection(
            viewer.current,
            tabId,
            comparison.root,
            JSON.stringify({
              base: comparison.base,
              target: comparison.target ?? 'working directory',
              mode: comparison.config.mode,
            }),
          );
        }}
      />
      <Button type="submit">{t('Compare')}</Button>
      <Button type="button" className="diff-refresh" onClick={refresh}>
        <span className="diff-spinner-slot">
          {loading && (
            <span className="diff-spinner" role="status" aria-label={t('Loading comparison')} />
          )}
        </span>
        {t('Refresh')}
      </Button>
    </form>
  );
}
