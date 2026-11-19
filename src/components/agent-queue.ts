import type { AgentActivity } from '../sdk';

export type QueueActivity = AgentActivity | 'unknown' | 'unavailable';
export type QueueMessage = {
  id: string;
  sourceId: number;
  text: string;
  label: string;
  threadKey?: string;
  state: 'pending' | 'sending' | 'uncertain';
};
export type AgentQueueState = {
  sessionId: number;
  targetId: number;
  items: QueueMessage[];
  activity: QueueActivity;
  name?: string;
  observed: boolean;
  phase: 'ready' | 'awaiting-work' | 'working';
  paused?: 'manual' | 'restored' | 'unavailable' | 'ambiguous' | 'storage';
  lastSent?: number;
};
type QueueStorage = {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
};
export type QueuePort = {
  observe: (
    sessionId: number,
    targetId: number,
    changed: (activity: QueueActivity, name?: string) => void,
  ) => () => void;
  send: (
    sessionId: number,
    targetId: number,
    item: QueueMessage,
    forced: boolean,
  ) => Promise<QueueActivity | undefined>;
  acknowledged?: (sessionId: number, targetId: number, item: QueueMessage) => void;
};
const key = 'ongleet.agent-queues.v1';
const byteLength = (text: string) => new TextEncoder().encode(text).length;

/** One durable FIFO per recipient. Only explicit human messages enter this queue. */
export class AgentQueues {
  private queues = new Map<string, AgentQueueState>();
  private ports = new Map<number, { sessionId: number; port: QueuePort }>();
  private observers = new Map<string, { sourceId: number; stop: () => void }>();
  private flights = new Map<string, Promise<void>>();
  private listeners = new Set<() => void>();
  private revision = 0;
  private targetRevisions = new Map<string, number>();
  private storageFailed = false;
  constructor(
    private storage: QueueStorage,
    private newId: () => string,
  ) {
    try {
      const raw = storage.getItem(key);
      if (!raw) return;
      if (raw.length > 1024 * 1024) throw new Error('Queue storage exceeds limit');
      const records: unknown = JSON.parse(raw);
      if (!Array.isArray(records) || records.length > 64) throw new Error('Invalid queue storage');
      let count = 0;
      const messageIds = new Set<string>();
      for (const record of records) {
        if (
          !record ||
          !Number.isSafeInteger(record.sessionId) ||
          record.sessionId <= 0 ||
          !Number.isSafeInteger(record.targetId) ||
          record.targetId <= 0 ||
          !Array.isArray(record.items) ||
          record.items.length > 20
        )
          throw new Error('Invalid queue record');
        for (const item of record.items) {
          if (
            !item ||
            typeof item.id !== 'string' ||
            item.id.length > 100 ||
            !Number.isSafeInteger(item.sourceId) ||
            item.sourceId <= 0 ||
            typeof item.text !== 'string' ||
            !item.text.trim() ||
            byteLength(item.text) > 8191 ||
            typeof item.label !== 'string' ||
            item.label.length > 300 ||
            (item.threadKey !== undefined &&
              (typeof item.threadKey !== 'string' || item.threadKey.length > 200)) ||
            !['pending', 'sending', 'uncertain'].includes(item.state)
          )
            throw new Error('Invalid queued message');
          if (messageIds.has(item.id)) throw new Error('Duplicate queued message');
          messageIds.add(item.id);
          item.state = item.state === 'pending' ? 'pending' : 'uncertain';
          if (++count > 64) throw new Error('Too many queued messages');
        }
        if (this.queues.has(this.key(record.sessionId, record.targetId)))
          throw new Error('Duplicate agent queue');
        this.queues.set(this.key(record.sessionId, record.targetId), {
          sessionId: record.sessionId,
          targetId: record.targetId,
          items: record.items,
          activity: 'unknown',
          observed: false,
          phase: 'ready',
          paused: 'restored',
        });
      }
    } catch {
      // Never overwrite unreadable saved messages or silently resume them.
      this.queues.clear();
      this.storageFailed = true;
    }
  }
  private key(sessionId: number, targetId: number) {
    return `${sessionId}.${targetId}`;
  }
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  version = () => this.revision;
  targetVersion(sessionId: number, targetId: number) {
    return this.targetRevisions.get(this.key(sessionId, targetId)) ?? 0;
  }
  subscribeTarget(sessionId: number, targetId: number, listener: () => void) {
    let version = this.targetVersion(sessionId, targetId);
    return this.subscribe(() => {
      const current = this.targetVersion(sessionId, targetId);
      if (current !== version) {
        version = current;
        listener();
      }
    });
  }
  pendingTargets(sessionId: number) {
    return JSON.stringify(this.list(sessionId).map((queue) => [queue.targetId, queue.name]));
  }
  private publish(id?: string) {
    for (const target of id ? [id] : this.queues.keys())
      this.targetRevisions.set(target, (this.targetRevisions.get(target) ?? 0) + 1);
    this.revision++;
    for (const listener of this.listeners) listener();
  }
  get(sessionId: number, targetId: number) {
    return this.queues.get(this.key(sessionId, targetId));
  }
  list(sessionId: number) {
    return [...this.queues.values()].filter(
      (queue) => queue.sessionId === sessionId && queue.items.length > 0,
    );
  }
  ensure(sessionId: number, targetId: number) {
    const id = this.key(sessionId, targetId);
    if (!this.queues.has(id))
      this.queues.set(id, {
        sessionId,
        targetId,
        items: [],
        activity: 'unknown',
        observed: false,
        phase: 'ready',
        paused: this.storageFailed ? 'storage' : undefined,
      });
    this.bind(id);
    return this.queues.get(id)!;
  }
  private save() {
    if (this.storageFailed) throw new Error('Saved agent queues are unavailable.');
    this.storage.setItem(
      key,
      JSON.stringify([...this.queues.values()].filter((queue) => queue.items.length)),
    );
  }
  attach(sourceId: number, sessionId: number, port: QueuePort) {
    this.ports.set(sourceId, { sessionId, port });
    for (const id of this.queues.keys()) this.bind(id);
    return () => {
      if (this.ports.get(sourceId)?.port !== port) return;
      this.ports.delete(sourceId);
      for (const [id, observer] of this.observers) {
        if (observer.sourceId !== sourceId) continue;
        observer.stop();
        this.observers.delete(id);
        const queue = this.queues.get(id)!;
        queue.activity = 'unknown';
        queue.observed = false;
        this.bind(id);
      }
      this.publish();
    };
  }
  private client(queue: AgentQueueState) {
    return [...this.ports.entries()].find(
      ([id, value]) => value.sessionId === queue.sessionId && id !== queue.targetId,
    );
  }
  private bind(id: string) {
    if (this.observers.has(id)) return;
    const queue = this.queues.get(id)!;
    const client = this.client(queue);
    if (!client) return;
    const [sourceId, { port }] = client;
    // Register ownership before observing: a test/native adapter can report synchronously.
    const observer = { sourceId, stop: () => {} };
    this.observers.set(id, observer);
    observer.stop = port.observe(queue.sessionId, queue.targetId, (activity, name) => {
      if (this.observers.get(id) !== observer) return;
      if (
        queue.observed &&
        queue.activity === activity &&
        (!name || queue.name === name.slice(0, 300))
      )
        return;
      queue.activity = activity;
      queue.observed = true;
      if (name) queue.name = name.slice(0, 300);
      if (activity === 'working' || activity === 'question' || activity === 'approval') {
        if (queue.phase === 'awaiting-work') queue.phase = 'working';
      } else if (activity === 'idle' && queue.phase === 'working') queue.phase = 'ready';
      if (activity === 'unknown' || activity === 'unavailable' || activity === 'stopped') {
        if (queue.items.length && !queue.paused) queue.paused = 'unavailable';
      }
      this.publish(id);
      void this.pump(id);
    });
  }
  enqueue(sessionId: number, targetId: number, message: Omit<QueueMessage, 'id' | 'state'>) {
    if (!message.text.trim() || byteLength(message.text) > 8191)
      throw new Error('Queued message must fit within 8 KiB including Enter.');
    const queue = this.ensure(sessionId, targetId);
    const total = [...this.queues.values()].reduce((sum, queue) => sum + queue.items.length, 0);
    if (queue.items.length >= 20 || total >= 64) throw new Error('Agent queue is full.');
    const item: QueueMessage = {
      ...message,
      label: message.label.slice(0, 300),
      id: this.newId(),
      state: 'pending',
    };
    queue.items.push(item);
    const previousPause = queue.paused;
    if (
      queue.observed &&
      ['unknown', 'unavailable', 'stopped'].includes(queue.activity) &&
      !queue.paused
    )
      queue.paused = 'unavailable';
    try {
      this.save();
    } catch (error) {
      queue.items.pop();
      queue.paused = previousPause;
      throw error;
    }
    this.publish(this.key(sessionId, targetId));
    void this.pump(this.key(sessionId, targetId));
    return item.id;
  }
  edit(sessionId: number, targetId: number, itemId: string, text: string) {
    if (!text.trim() || byteLength(text) > 8191)
      throw new Error('Queued message must fit within 8 KiB including Enter.');
    const queue = this.ensure(sessionId, targetId);
    const item = queue.items.find((item) => item.id === itemId);
    if (!item || item.state !== 'pending') throw new Error('This message cannot be edited.');
    const old = item.text;
    item.text = text;
    try {
      this.save();
    } catch (error) {
      item.text = old;
      throw error;
    }
    this.publish(this.key(sessionId, targetId));
  }
  remove(sessionId: number, targetId: number, itemId: string) {
    const queue = this.ensure(sessionId, targetId);
    const index = queue.items.findIndex((item) => item.id === itemId);
    if (index < 0 || queue.items[index].state === 'sending')
      throw new Error('This message cannot be removed.');
    const [item] = queue.items.splice(index, 1);
    try {
      this.save();
    } catch (error) {
      queue.items.splice(index, 0, item);
      throw error;
    }
    this.publish(this.key(sessionId, targetId));
  }
  pause(sessionId: number, targetId: number) {
    const queue = this.ensure(sessionId, targetId);
    queue.paused = 'manual';
    this.save();
    this.publish(this.key(sessionId, targetId));
  }
  resume(sessionId: number, targetId: number) {
    const queue = this.ensure(sessionId, targetId);
    if (queue.items.some((item) => item.state === 'uncertain'))
      throw new Error('Remove the uncertain message before resuming.');
    const previousPause = queue.paused;
    const previousPhase = queue.phase;
    if (queue.paused === 'restored' || queue.paused === 'ambiguous') queue.phase = 'ready';
    queue.paused = undefined;
    try {
      this.save();
    } catch (error) {
      queue.paused = previousPause;
      queue.phase = previousPhase;
      throw error;
    }
    this.publish(this.key(sessionId, targetId));
    void this.pump(this.key(sessionId, targetId));
  }
  sendNow(sessionId: number, targetId: number, itemId: string) {
    const queue = this.ensure(sessionId, targetId);
    if (queue.items.some((item) => item.state === 'uncertain'))
      throw new Error('Remove the uncertain message before resuming.');
    return this.pump(this.key(sessionId, targetId), itemId);
  }
  private async pump(id: string, forceId?: string) {
    const queue = this.queues.get(id)!;
    if (this.flights.has(id)) return;
    if (!forceId && (queue.paused || queue.phase !== 'ready' || queue.activity !== 'idle')) return;
    const item = forceId ? queue.items.find((item) => item.id === forceId) : queue.items[0];
    const client = this.client(queue);
    if (!item || item.state !== 'pending' || !client) return;
    item.state = 'sending';
    const previousPhase = queue.phase;
    queue.phase = 'awaiting-work';
    try {
      this.save();
    } catch {
      item.state = 'pending';
      queue.phase = previousPhase;
      queue.paused = 'storage';
      this.publish(id);
      return;
    }
    // Install the lock before entering the adapter, including synchronous metadata callbacks.
    let complete!: () => void;
    const flight = new Promise<void>((resolve) => {
      complete = resolve;
    });
    this.flights.set(id, flight);
    this.publish(id);
    try {
      const activity = await client[1].port.send(queue.sessionId, queue.targetId, item, !!forceId);
      if (activity !== undefined) {
        // Preflight observed a newly busy agent; no write took place.
        item.state = 'pending';
        queue.phase = previousPhase;
        queue.activity = activity;
        if (['unknown', 'unavailable', 'stopped'].includes(activity) && !queue.paused)
          queue.paused = 'unavailable';
        return;
      }
      queue.items = queue.items.filter((candidate) => candidate.id !== item.id);
      queue.lastSent = Date.now();
      try {
        client[1].port.acknowledged?.(queue.sessionId, queue.targetId, item);
      } catch {
        /* Acknowledged writes stay acknowledged. */
      }
    } catch {
      item.state = 'uncertain';
      queue.paused = 'ambiguous';
    } finally {
      try {
        this.save();
      } catch {
        queue.paused = 'storage';
      }
      this.flights.delete(id);
      complete();
      this.publish(id);
    }
    // A complete work/idle cycle can arrive before the write acknowledgement.
    void this.pump(id);
  }
  async shutdown() {
    for (const queue of this.queues.values()) if (queue.items.length) queue.paused = 'manual';
    try {
      this.save();
    } catch {
      /* The saved sending marker already prevents replay after restart. */
    }
    this.publish();
    await Promise.all(this.flights.values());
  }
}
