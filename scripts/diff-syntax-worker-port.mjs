import { Worker as NodeWorker } from 'node:worker_threads';

// Run the production worker entrypoint in a real thread, adapting only browser messaging.
export class ThreadPort {
  constructor() {
    this.thread = new NodeWorker(
      `
      const { parentPort } = require('node:worker_threads');
      globalThis.self = { postMessage: data => parentPort.postMessage(data) };
      import(${JSON.stringify(new URL('../src/plugins/diff/src/syntax-worker.ts', import.meta.url).href)})
        .then(() => parentPort.on('message', data => self.onmessage({ data })));
    `,
      { eval: true, execArgv: [] },
    );
    this.thread.on('message', (data) => this.onmessage?.({ data }));
    this.thread.on('error', (error) => this.onerror?.(error));
  }
  postMessage(data) {
    this.thread.postMessage(data);
  }
  terminate() {
    void this.thread.terminate();
  }
}
