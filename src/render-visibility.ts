import type { CanvasRenderer } from 'ghostty-web';

// ghostty-web 0.4.0 exposes its renderer but has no terminal visibility API.
// Gate all drawing calls, including upstream scroll/scrollbar animations,
// without stopping synchronous VT writes. Scheduling lives in terminal-rendering.
export function suppressHiddenRendering(
  renderer: Pick<CanvasRenderer, 'render'>,
  visible: () => boolean,
) {
  const render = renderer.render.bind(renderer);
  renderer.render = (...args) => {
    if (visible()) render(...args);
  };
}
