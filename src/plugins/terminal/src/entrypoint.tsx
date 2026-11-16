import { TerminalContent } from './Content';
import { TerminalConfigurationEditor, terminalDraft, terminalConfig } from './ConfigurationEditor';
import { TerminalFrontend } from './frontend';
export default {
  background: true,
  Content: TerminalContent,
  ConfigurationEditor: TerminalConfigurationEditor,
  toDraft: terminalDraft,
  fromDraft: terminalConfig,
  beforeQuit: () => TerminalFrontend.saveAllHistory(),
};
