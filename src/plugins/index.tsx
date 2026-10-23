import type { ComponentType } from 'react';
import type { PluginContentProps } from './types';
import { PullRequestContent, PullRequestConfigurationEditor } from './pull-request/Content';
import { FilesContent } from './files/Content';
import { FilesConfigurationEditor } from './files/ConfigurationEditor';
import {
  TerminalConfigurationEditor,
  launchDraft,
  launchConfig,
} from './terminal/ConfigurationEditor';
import { TerminalContent } from './terminal/Content';
import { TerminalFrontend } from './terminal/frontend';

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
  beforeQuit?: () => Promise<void>;
};
export const plugins: TabPlugin[] = [
  {
    id: 'terminal',
    label: 'Terminal',
    configVersion: 1,
    Content: TerminalContent,
    ConfigurationEditor: TerminalConfigurationEditor,
    toDraft: launchDraft,
    fromDraft: launchConfig,
    beforeQuit: () => TerminalFrontend.saveAllHistory(),
  },
  {
    id: 'files',
    label: 'Files',
    configVersion: 1,
    Content: FilesContent,
    ConfigurationEditor: FilesConfigurationEditor,
    toDraft: JSON.stringify,
    fromDraft: JSON.parse,
  },
  {
    id: 'pull-request',
    label: 'Pull request',
    configVersion: 1,
    Content: PullRequestContent,
    ConfigurationEditor: PullRequestConfigurationEditor,
    toDraft: JSON.stringify,
    fromDraft: JSON.parse,
  },
];
export async function flushPlugins() {
  await Promise.all(plugins.map((plugin) => plugin.beforeQuit?.()));
}
export function TabHost(props: PluginContentProps) {
  const plugin = plugins.find((plugin) => plugin.id === props.tab.kind);
  if (plugin && plugin.configVersion === props.tab.configVersion)
    return <plugin.Content {...props} />;
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
