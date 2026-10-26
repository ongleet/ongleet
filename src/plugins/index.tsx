import { useEffect, useState, type ComponentType } from 'react';
import { createSdk } from '../sdk/transport';
import { SdkContext, type PluginSdk } from '../sdk';
import type { PluginContentProps } from './types';
import {
  TerminalConfigurationEditor,
  launchDraft,
  launchConfig,
} from './terminal/ConfigurationEditor';

export type ConfigurationEditorProps = { value: string; onChange: (value: string) => void };
export type TabPlugin = {
  id: string;
  label: string;
  configVersion: number;
  // React attachment/cleanup effects and the visibility prop form the content lifecycle.
  Content: ComponentType<PluginContentProps>;
  ConfigurationEditor: ComponentType<ConfigurationEditorProps>;
  toDraft: (config: unknown) => string;
  fromDraft: (draft: string) => unknown;
  contentConfiguration?: boolean;
  beforeQuit?: () => Promise<void>;
};
const manifests = import.meta.glob<{
  default: {
    id: string;
    label: string;
    configVersion: number;
    sdkVersion: number;
    entrypoint: string;
  };
}>('./*/manifest.json', { eager: true });
const entrypoints = import.meta.glob<{
  default: Omit<TabPlugin, 'id' | 'label' | 'configVersion'>;
}>('./*/entrypoint.tsx', { eager: true });
export const plugins: TabPlugin[] = Object.entries(manifests).flatMap(
  ([path, { default: manifest }]) => {
    const entrypoint = entrypoints[path.replace('manifest.json', manifest.entrypoint)];
    return manifest.sdkVersion === 1 && entrypoint ? [{ ...manifest, ...entrypoint.default }] : [];
  },
);
export async function flushPlugins() {
  await Promise.all(plugins.map((plugin) => plugin.beforeQuit?.()));
}
type HostProps = PluginContentProps & { reportTitle: (id: number, title: string | null) => void };
export function TabHost(props: HostProps) {
  const plugin = plugins.find((plugin) => plugin.id === props.tab.kind);
  if (plugin && plugin.configVersion === props.tab.configVersion)
    return <AttachedPlugin key={props.tab.id} plugin={plugin} props={props} />;
  return (
    <section
      role="tabpanel"
      id={`tab-panel-${props.tab.id}`}
      aria-labelledby={`tab-${props.tab.id}`}
      hidden={!props.visible}
      className="plugin-panel"
    >
      <p>
        Plugin unavailable: {props.tab.kind} (configuration version {props.tab.configVersion}). Its
        saved configuration has been retained.
      </p>
    </section>
  );
}

// External commands reuse the launch form but remain outside the tab registry.
export const commandEditor = {
  ConfigurationEditor: TerminalConfigurationEditor,
  toDraft: launchDraft,
  fromDraft: launchConfig,
};
export function actionEditor(kind: string) {
  return kind === 'command' ? commandEditor : plugins.find((plugin) => plugin.id === kind);
}

function AttachedPlugin({ plugin, props }: { plugin: TabPlugin; props: HostProps }) {
  const [sdk, setSdk] = useState<PluginSdk>();
  const [error, setError] = useState('');
  useEffect(() => {
    const instance = createSdk(props.tab.id, (title) => props.reportTitle(props.tab.id, title));
    let active = true;
    let unregister: (() => void) | undefined;
    const validationReady =
      import.meta.env.MODE === 'native-validation'
        ? import('../native-validation').then(({ registerSdk }) => {
            if (active) unregister = registerSdk(props.tab.id, instance);
          })
        : Promise.resolve();
    void Promise.all([instance.ready, validationReady])
      .then(() => {
        if (active) setSdk(instance);
      })
      .catch((error) => {
        if (active) setError(String(error));
      });
    return () => {
      active = false;
      unregister?.();
      void instance.dispose().catch(console.error);
    };
  }, [props.tab.id]);
  if (!sdk) return error ? <p role="alert">{error}</p> : null;
  return (
    <SdkContext.Provider value={sdk}>
      <plugin.Content {...props} />
    </SdkContext.Provider>
  );
}
