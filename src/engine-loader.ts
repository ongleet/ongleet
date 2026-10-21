// Share an in-flight/successful engine load across tabs, but do not retain a
// failed request: a development server or asset may be temporarily unavailable.
export function createEngineLoader<T>(load: () => Promise<T>): () => Promise<T> {
  let pending: Promise<T> | undefined;
  return () => {
    pending ??= Promise.resolve().then(load).catch(error => {
      pending = undefined;
      throw error;
    });
    return pending;
  };
}
