import { useEffect, useRef } from 'react';

let enabled = false;
let nextInstance = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
const pending: object[] = [];

/** Bounded console batches; never include input text, paths or prop values. */
export function renderDiagnostic(event: string, details: object = {}) {
  if (!enabled || pending.length >= 128) return;
  pending.push({ atMs: Math.round(performance.now()), event, ...details });
  timer ??= setTimeout(() => {
    timer = undefined;
    console.info('[diff-diagnostics]', JSON.stringify(pending.splice(0)));
  }, 500);
}

export function setRenderDiagnosticsEnabled(value: boolean) {
  enabled = value;
  renderDiagnostic('enabled');
}

/** Counts committed renders, including parent renders with unchanged tracked inputs. */
export function useRenderDiagnostic(component: string, inputs: Record<string, unknown>) {
  const instance = useRef(0);
  const previous = useRef<Record<string, unknown> | undefined>(undefined);
  const renders = useRef(0);
  useEffect(() => {
    if (!enabled) return;
    instance.current ||= ++nextInstance;
    const changed = previous.current
      ? Object.keys(inputs).filter((key) => !Object.is(inputs[key], previous.current![key]))
      : ['mount-or-diagnostics-enabled'];
    renderDiagnostic('render-committed', {
      component,
      instance: instance.current,
      render: ++renders.current,
      changed: changed.length ? changed : ['parent-or-untracked-input'],
    });
    previous.current = inputs;
  });
}
