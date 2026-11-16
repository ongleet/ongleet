/** Keep scroll-driven selection outside the comparison render state. */
export class DiffSelection {
  private selected = '';
  private listeners = new Set<() => void>();

  getSnapshot = () => this.selected;

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  set = (value: string) => {
    if (value === this.selected) return;
    this.selected = value;
    for (const listener of this.listeners) listener();
  };
}
