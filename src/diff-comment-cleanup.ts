const prefix = 'ongleet.diff-comments.v1.';
export function reconcileCommentSessions(ids: number[]) {
  for (const key of Object.keys(localStorage)) {
    if (key.startsWith(prefix) && !ids.includes(Number(key.slice(prefix.length).split('.')[0])))
      localStorage.removeItem(key);
  }
}
