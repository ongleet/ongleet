import type { CanvasRenderer } from 'ghostty-web';

// ghostty-web 0.4.0 exposes its renderer but has no terminal visibility API.
// Gate its public rendering boundary without stopping synchronous VT writes or
// using the private animation-loop fields. Cursor polling remains upstream.
export function suppressHiddenRendering(
  renderer: Pick<CanvasRenderer, 'render'>,
  visible: () => boolean,
) {
  const render = renderer.render.bind(renderer);
  renderer.render = (...args) => {
    if (visible()) render(...args);
  };
}
