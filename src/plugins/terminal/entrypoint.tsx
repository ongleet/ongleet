import { TerminalContent } from './Content';
import { TerminalConfigurationEditor, launchDraft, launchConfig } from './ConfigurationEditor';
import { TerminalFrontend } from './frontend';
export default {
  Content: TerminalContent,
  ConfigurationEditor: TerminalConfigurationEditor,
  toDraft: launchDraft,
  fromDraft: launchConfig,
  beforeQuit: () => TerminalFrontend.saveAllHistory(),
};
