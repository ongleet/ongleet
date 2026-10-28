import { DiffContent } from './Content';
import { ConfigurationEditor } from './ConfigurationEditor';
import { settings } from './settings';
export default {
  Content: DiffContent,
  ConfigurationEditor,
  toDraft: JSON.stringify,
  fromDraft: (draft: string) => settings(JSON.parse(draft)),
};
