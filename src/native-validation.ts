// Imported only by the explicit native-validation Vite build. The normal
// application bundle exposes no testing bridge or terminal handles.
type Probe = {
  state: () => { sessionId?: number; replaying: boolean; cols?: number; rows?: number; scrollback?: number; alternate?: boolean };
  snapshot: () => string;
  selectAll: () => void;
};
const terminals = new Map<number, Probe>();
export function registerTerminal(id: number, probe: Probe) {
  terminals.set(id, probe);
  return () => { if (terminals.get(id) === probe) terminals.delete(id); };
}
Object.assign(window, {
  __terminalValidation: {
    state: (id: number) => terminals.get(id)?.state(),
    snapshot: (id: number) => terminals.get(id)?.snapshot(),
    selectAll: (id: number) => terminals.get(id)?.selectAll(),
  },
});
