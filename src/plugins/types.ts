import type { ProcessStatus } from '../terminal';
export type PluginTab = {
  id: number;
  kind: string;
  configVersion: number;
  isOpen: boolean;
  resumeOnStart: boolean;
  name: string;
  displayOrder: number;
  config: unknown;
};
export type PluginContentProps = {
  tab: PluginTab;
  session: { id: number; pullRequest: { repository: string; number: number } | null };
  visible: boolean;
  reportStatus: (id: number, status: ProcessStatus | null) => void;
  markRestoring: (delta: number) => void;
};
