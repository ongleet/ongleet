import { Button } from '../../../../components/Controls';
import { t } from '../../../../i18n';
import { SdkError } from '../../../../sdk';
import { Composer } from './Discussions';
import type { DiffController } from '../hooks/useDiffController';

/** Reject draft submission when its captured comparison revision is no longer current. */
export function NewDiscussion({ controller }: { controller: DiffController }) {
  const {
    compose,
    changeComments,
    comments,
    setCompose,
    loading,
    error,
    composeRevision,
    reviews,
  } = controller;
  return (
    compose && (
      <div className="diff-discussion diff-new-thread">
        <p>
          {compose.path} · {compose.side ?? t('file')} {compose.line ?? ''}
        </p>
        <Composer
          autoFocus
          submit={(text) => {
            changeComments(() => {
              if (loading || error || composeRevision.current !== reviews.current.revision)
                throw new SdkError(
                  'revision_conflict',
                  'Comparison changed; reopen the comment control',
                );
              return comments.add(compose, text, 'User');
            });
            setCompose(undefined);
          }}
        />
        <Button onClick={() => setCompose(undefined)}>{t('Cancel')}</Button>
      </div>
    )
  );
}
