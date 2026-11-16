import { SessionConfiguration } from './SessionConfiguration';
import { FilesContent } from './Content';
import { FilesConfigurationEditor } from './ConfigurationEditor';
export default {
  Content: FilesContent,
  SessionConfiguration,
  ConfigurationEditor: FilesConfigurationEditor,
  toDraft: JSON.stringify,
  fromDraft: JSON.parse,
};
