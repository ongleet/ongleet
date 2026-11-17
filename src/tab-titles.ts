import type { PluginTab } from './plugins/types';

type Titles = Record<number, string>;
const key = (sessionId: number) => `ongleet.tab-titles.v1:${sessionId}`;

// Keep only titles belonging to open tabs in this session, including lazy tabs.
export function openTabTitles(value: unknown, tabs: PluginTab[]): Titles {
  const titles: Titles = {};
  if (!value || typeof value !== 'object') return titles;
  for (const tab of tabs) {
    if (!tab.isOpen) continue;
    const title = (value as Titles)[tab.id];
    if (typeof title !== 'string' || title.length > 256) continue;
    const clean = title.replace(/[\u0000-\u001f\u007f-\u009f]/g, '').trim();
    if (clean) titles[tab.id] = clean;
  }
  return titles;
}

export function loadTabTitles(sessionId: number, tabs: PluginTab[]): Titles {
  try {
    return openTabTitles(JSON.parse(localStorage.getItem(key(sessionId)) ?? '{}'), tabs);
  } catch {
    return {};
  }
}

export function saveTabTitles(sessionId: number, tabs: PluginTab[], titles: Titles): void {
  try {
    const current = openTabTitles(titles, tabs);
    if (Object.keys(current).length) localStorage.setItem(key(sessionId), JSON.stringify(current));
    else localStorage.removeItem(key(sessionId));
  } catch {
    // Storage failures must not prevent live title updates or tab navigation.
  }
}
