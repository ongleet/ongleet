import { FilesContent } from './Content';
import { FilesConfigurationEditor } from './ConfigurationEditor';
export default {
  Content: FilesContent,
  ConfigurationEditor: FilesConfigurationEditor,
  toDraft: JSON.stringify,
  fromDraft: JSON.parse,
};
