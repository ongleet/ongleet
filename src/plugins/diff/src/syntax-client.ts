import type { HunkData, HunkTokens } from 'react-diff-view';
import type { HighlightResponse } from './syntax-worker';

type WorkerPort = Pick<
  Worker,
  'postMessage' | 'terminate' | 'onmessage' | 'onerror' | 'onmessageerror'
>;
type Job = {
  id: number;
  key: string;
  filename: string;
  hunks: HunkData[];
  listeners: Set<(tokens: HunkTokens | undefined) => void>;
};

/** Serialize parser work through one lazy worker; cache only small completed patches. */
export class SyntaxClient {
  private worker?: WorkerPort;
  private active?: Job;
  private jobs = new Map<string, Job>();
  private cache = new Map<string, HunkTokens>();
  private sequence = 0;
  private deadline?: ReturnType<typeof setTimeout>;
  private idle?: ReturnType<typeof setTimeout>;

  private createWorker: () => WorkerPort;

  constructor(
    createWorker: () => WorkerPort = () =>
      new Worker(new URL('./syntax-worker.ts', import.meta.url), { type: 'module' }),
  ) {
    this.createWorker = createWorker;
  }

  request(filename: string, patch: string, hunks: HunkData[]) {
    const key = `${filename}\0${patch}`;
    const cached = this.cache.get(key);
    if (cached) {
      this.cache.delete(key);
      this.cache.set(key, cached);
      return { result: Promise.resolve(cached), cancel() {} };
    }
    let job = this.jobs.get(key);
    if (!job) {
      job = { id: ++this.sequence, key, filename, hunks, listeners: new Set() };
      this.jobs.set(key, job);
    }
    const pending = job;
    let settle!: (tokens: HunkTokens | undefined) => void;
    const result = new Promise<HunkTokens | undefined>((resolve) => {
      settle = resolve;
      pending.listeners.add(resolve);
    });
    this.start();
    return {
      result,
      cancel: () => {
        if (!pending.listeners.delete(settle)) return;
        settle(undefined);
        if (pending.listeners.size) return;
        this.jobs.delete(key);
        if (this.active === pending) this.stopWorker();
        this.start();
      },
    };
  }

  private start() {
    clearTimeout(this.idle);
    if (this.active) return;
    const job = this.jobs.values().next().value;
    if (!job) {
      // Reuse loaded language modules during bursts, then release the worker.
      if (this.worker) this.idle = setTimeout(() => this.stopWorker(), 10_000);
      return;
    }
    this.active = job;
    try {
      if (!this.worker) {
        const worker = this.createWorker();
        this.worker = worker;
        worker.onmessage = (event: MessageEvent<HighlightResponse>) => {
          if (this.worker !== worker || event.data.id !== this.active?.id) return;
          this.finish(event.data.tokens);
        };
        worker.onerror = worker.onmessageerror = () => {
          if (this.worker !== worker) return;
          this.fail();
        };
      }
      this.deadline = setTimeout(() => this.fail(), 10_000);
      this.worker.postMessage({ id: job.id, filename: job.filename, hunks: job.hunks });
    } catch {
      this.fail();
    }
  }

  private finish(tokens?: HunkTokens) {
    clearTimeout(this.deadline);
    const job = this.active!;
    this.active = undefined;
    this.jobs.delete(job.key);
    // At most sixteen <=64 KiB patches; large manual expansions are not retained.
    if (tokens && job.key.length <= 32 * 1024) {
      this.cache.set(job.key, tokens);
      if (this.cache.size > 16) this.cache.delete(this.cache.keys().next().value!);
    }
    for (const listener of job.listeners) listener(tokens);
    job.listeners.clear();
    this.start();
  }

  private fail() {
    // A worker startup/crash/timeout failure leaves every queued patch readable.
    const jobs = [...this.jobs.values()];
    this.jobs.clear();
    this.stopWorker();
    for (const job of jobs) {
      for (const listener of job.listeners) listener(undefined);
      job.listeners.clear();
    }
  }

  private stopWorker() {
    clearTimeout(this.deadline);
    clearTimeout(this.idle);
    this.worker?.terminate();
    this.worker = undefined;
    this.active = undefined;
  }
}

export const syntaxClient = new SyntaxClient();
