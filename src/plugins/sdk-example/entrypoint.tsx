import { useState } from 'react';
import { useSdk } from '../../sdk';
import type { PluginContentProps } from '../types';
function Content({ tab, visible }: PluginContentProps) {
  const sdk = useSdk();
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  async function run() {
    setBusy(true);
    try {
      const folder = (tab.config as { folder: string }).folder;
      const listing = await sdk.fs.list({
        folder,
        offset: 0,
        order: 'nameAscending',
        showHidden: false,
        query: '',
      });
      const result = await sdk.process.run({
        executable: '/usr/bin/printf',
        args: ['%s', 'Literal SDK example: $(never executed)'],
      });
      if (result.code !== 0) throw new Error(result.stderr);
      const previous = await sdk.storage.read<{ runs: number }>('tab');
      const saved = await sdk.storage.replace('tab', previous?.revision ?? 0, 1, {
        runs: (previous?.value.runs ?? 0) + 1,
      });
      setMessage(
        `${result.stdout}; ${listing.entries.length} entries; saved run ${saved.value.runs}`,
      );
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="plugin-panel" role="tabpanel" id={`tab-panel-${tab.id}`} hidden={!visible}>
      <p>
        Frontend-only filesystem, literal process arguments and revision-checked storage example.
      </p>
      <button disabled={busy} onClick={() => void run()}>
        Run SDK example
      </button>
      <p role="status">{message}</p>
    </section>
  );
}
function ConfigurationEditor() {
  return <p>Uses the session directory by default.</p>;
}
export default { Content, ConfigurationEditor, toDraft: JSON.stringify, fromDraft: JSON.parse };
