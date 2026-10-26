export type ProcessStatus = { state: 'running' } | { state: 'exited'; code: number };
export type PtyOutput =
  | { type: 'data'; seq: number; bytes: number[] }
  | { type: 'size'; seq: number; cols: number; rows: number }
  | { type: 'ready'; seq: number }
  | { type: 'eof' }
  | { type: 'error'; message: string };
export type Attachment = { id: number; cols: number; rows: number; replay: boolean };
export type TextDocument = {
  path: string;
  text: string;
  revision: string;
  lineEnding: string;
  bom: boolean;
};
export type Listing = {
  folder: string;
  parent: string | null;
  entries: { name: string; path: string; directory: boolean; symlink: boolean }[];
  nextOffset: number | null;
};
export type ProcessRequest = {
  executable: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  timeoutMs?: number;
  outputLimit?: number;
};
export type ProcessResult = { stdout: string; stderr: string; code: number };
export type Scope = 'session' | 'tab';
export type Namespace<T = unknown> = { version: number; revision: number; value: T };
export class SdkError extends Error {
  constructor(
    public readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = 'SdkError';
  }
}

export type SortOrder =
  | 'nameAscending'
  | 'nameDescending'
  | 'dateAscending'
  | 'dateDescending'
  | 'sizeAscending'
  | 'sizeDescending';

export type SessionTag = { name: string; background?: string; dot?: string; priority?: number };

export type HttpRequest = {
  url: string;
  method: string;
  headers?: Record<string, string>;
  body?: string;
  signal?: AbortSignal;
};
export type HttpResponse = { status: number; headers: Record<string, string>; body: string };
