import { PullRequestContent, PullRequestConfigurationEditor } from './Content';
import { SessionConfiguration } from './SessionConfiguration';
import { refreshSeconds } from './settings';
export default {
  Content: PullRequestContent,
  SessionConfiguration,
  contentConfiguration: true,
  ConfigurationEditor: PullRequestConfigurationEditor,
  toDraft: JSON.stringify,
  fromDraft: (draft: string) => {
    const config = JSON.parse(draft);
    refreshSeconds(config);
    return config;
  },
};
