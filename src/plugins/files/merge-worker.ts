import { mergeText } from './editor-merge';

self.onmessage = (event: MessageEvent<{ original: string; local: string; disk: string }>) => {
  try {
    const { original, local, disk } = event.data;
    self.postMessage({ result: mergeText(original, local, disk) });
  } catch (error) {
    self.postMessage({ error: String(error) });
  }
};
