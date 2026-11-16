import { useEffect } from 'react';
import { useSdk } from '../../../sdk';
import { setRenderDiagnosticsEnabled, useRenderDiagnostic } from '../../../render-diagnostics';
export { renderDiagnostic as diffDiagnostic } from '../../../render-diagnostics';

let initialization: Promise<void> | undefined;
export function useDiffRenderDiagnostic(component: string, inputs: Record<string, unknown>) {
  const sdk = useSdk();
  useEffect(() => {
    initialization ??= sdk.env
      .get('ONGLEET_DEBUG')
      .then((value) => {
        setRenderDiagnosticsEnabled(value === '1');
      })
      .catch((error) => {
        initialization = undefined;
        console.warn('Render diagnostics initialization failed', error);
      });
  }, [sdk]);
  useRenderDiagnostic(component, inputs);
}
