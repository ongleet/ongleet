import { TerminalContent } from './Content';
import { TerminalConfigurationEditor, launchDraft, launchConfig } from './ConfigurationEditor';
import { TerminalFrontend } from './frontend';
export default {
  background: true,
  Content: TerminalContent,
  ConfigurationEditor: TerminalConfigurationEditor,
  toDraft: launchDraft,
  fromDraft: launchConfig,
  beforeQuit: () => TerminalFrontend.saveAllHistory(),
};
