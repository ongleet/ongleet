// Keep editors mounted across navigation, and guard explicit GUI destruction.
export const fileEditors = new Map<number, { path: string; dirty: boolean; busy: boolean }>();
export async function allowEditorClose(
  ids: number[],
  confirm: (message: string) => Promise<boolean>,
) {
  const editors = ids.flatMap((id) => fileEditors.get(id) ?? []);
  if (editors.some((editor) => editor.busy)) return false;
  const dirty = editors.filter((editor) => editor.dirty);
  return (
    dirty.length === 0 ||
    confirm(`Discard unsaved changes in ${dirty.map((editor) => editor.path).join(', ')}?`)
  );
}
