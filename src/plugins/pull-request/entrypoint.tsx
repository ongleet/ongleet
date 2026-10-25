import { PullRequestContent, PullRequestConfigurationEditor } from './Content';
import { refreshSeconds } from './settings';
export default {
  Content: PullRequestContent,
  ConfigurationEditor: PullRequestConfigurationEditor,
  toDraft: JSON.stringify,
  fromDraft: (draft: string) => {
    const config = JSON.parse(draft);
    refreshSeconds(config);
    return config;
  },
};
