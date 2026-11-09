import type { SortOrder } from '../../sdk';

export type BrowserPreferences = { showHidden: boolean; order: SortOrder };
export const browserDefaults: BrowserPreferences = { showHidden: false, order: 'nameAscending' };
