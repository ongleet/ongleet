import { PullRequestContent, PullRequestConfigurationEditor } from './Content';
export default {
  Content: PullRequestContent,
  ConfigurationEditor: PullRequestConfigurationEditor,
  toDraft: JSON.stringify,
  fromDraft: JSON.parse,
};
