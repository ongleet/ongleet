import { TabPanel } from '../../../components/Controls';
import { t } from '../../../i18n';
import type { PluginContentProps } from '../../types';
import { useDiffController } from './hooks/useDiffController';
import { useDiffInteractions } from './hooks/useDiffInteractions';
import { DiffToolbar } from './components/DiffToolbar';
import { DiffComparison } from './components/DiffComparison';
import { DiffStatusBar } from './components/DiffStatusBar';
import 'react-diff-view/style/index.css';
import '../assets/diff.css';
import '../../../components/file-tree.css';

/** Compose the Diff tab while keeping its controller alive across visibility changes. */
export function DiffContent(props: PluginContentProps) {
  const { tab, session, visible } = props;
  const controller = useDiffController(props);
  useDiffInteractions(controller);
  const { comparison, commentError, comments, tagError, error, discussion, filter, loading } =
    controller;
  return (
    <TabPanel
      hidden={!visible}
      className="plugin-panel diff-panel"
      id={`tab-panel-${tab.id}`}
      aria-labelledby={`tab-${tab.id}`}
    >
      <DiffToolbar
        controller={controller}
        sessionId={session.id}
        tabId={tab.id}
        visible={visible}
      />
      {(commentError || comments.error) && <p role="alert">{commentError || comments.error}</p>}
      {tagError && (
        <p role="alert">
          {t('Could not update the session change count: ')}
          {tagError}
        </p>
      )}
      {error && (
        <p role="alert">
          {error}
          {t(' Check the folder, Git installation and revision expressions, then Compare again.')}
        </p>
      )}
      {!comparison && comments.threads.length > 0 && (
        <section>
          <h3>{t('Saved comments · comparison unavailable')}</h3>
          {comments.threads.map(discussion)}
        </section>
      )}

      {comparison && (
        <DiffComparison controller={controller} comparison={comparison} visible={visible} />
      )}
      {comparison && (
        <DiffStatusBar comparison={comparison} filter={filter} loading={loading} error={error} />
      )}
    </TabPanel>
  );
}
