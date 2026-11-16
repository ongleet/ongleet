import { Input } from '../../../../components/Controls';
import { t } from '../../../../i18n';
import { FileTree } from './FileTree';
import { DiffSidebar } from './Sidebar';
import { DiffFile } from './DiffFile';
import { NewDiscussion } from './NewDiscussion';
import { DiffLoading } from '../loading-state';
import type { Comparison } from '../git';
import type { DiffController } from '../hooks/useDiffController';

/** Compose filename navigation, patch articles and orphan discussions around the retained viewport. */
export function DiffComparison({
  controller,
  comparison,
  visible,
}: {
  controller: DiffController;
  comparison: Comparison;
  visible: boolean;
}) {
  const { filter, setFilter, selection, select, viewer, loadingState, comments, discussion } =
    controller;
  const composer = <NewDiscussion controller={controller} />;
  const labels: Record<string, string> = {
    A: t('Added'),
    M: t('Modified'),
    D: t('Deleted'),
    R: t('Renamed'),
    T: t('Type changed'),
    C: t('Copied'),
    U: t('Unmerged'),
  };

  return (
    <div className="diff-layout">
      <DiffSidebar>
        <label>
          {t('Filter filenames')}
          <Input value={filter} onChange={(e) => setFilter(e.target.value)} />
        </label>
        <nav className="file-tree" aria-label={t('Changed files')}>
          <FileTree
            files={comparison.files.filter((f) =>
              `${f.oldPath}\n${f.path}`.toLowerCase().includes(filter.toLowerCase()),
            )}
            selection={selection}
            visible={visible}
            onSelect={select}
            labels={labels}
          />
        </nav>
        {!comparison.files.length && <p>{t('No changes.')}</p>}
      </DiffSidebar>
      <div ref={viewer} className="diff-viewer" tabIndex={0} aria-label={t('Unified diff')}>
        <DiffLoading.Provider value={loadingState}>
          <div className="diff-page">
            {comparison.files.map((file) => (
              <DiffFile
                controller={controller}
                comparison={comparison}
                file={file}
                labels={labels}
                composer={composer}
                key={JSON.stringify([
                  comparison.root,
                  comparison.config.mode,
                  comparison.config.baseRef,
                  comparison.config.targetRef,
                  file.key,
                ])}
              />
            ))}
            {comments.threads.some((t) => t.orphan) && (
              <section>
                <h3>{t('Orphan comments')}</h3>
                {comments.threads.filter((t) => t.orphan).map(discussion)}
              </section>
            )}
            {!comparison.files.length && <p>{t('No changes.')}</p>}
          </div>
        </DiffLoading.Provider>
      </div>
    </div>
  );
}
