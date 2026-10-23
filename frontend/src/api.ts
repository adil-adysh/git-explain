import type { WorkspaceMode } from './types';

export interface LineRange {
  start: number;
  end: number;
}

export interface AnnotationRegion {
  id: number;
  start_line: number;
  end_line: number;
  source: string;
}

export interface WorkspaceSection {
  id: string;
  name: string;
  kind: string;
  start_line: number;
  end_line: number;
  source_side: string;
  change_status: string;
  capabilities: string[];
  generation_id: string;
}

export interface WorkspaceFile {
  id: string;
  path_label: string;
  old_path_label?: string | null;
  status: string;
  language: string;
  section_count: number;
  sections: WorkspaceSection[];
}

export interface WorkspaceCapabilities {
  changes_view: boolean;
  learning_profiles: boolean;
  section_override: boolean;
}

export interface WorkspaceIndex {
  repository_label?: string;
  runtime?: { version: string; model: string };
  schema_version: number;
  mode: string;
  session_id?: string | null;
  generation: number;
  snapshot_id: string;
  revision_label: string;
  capabilities: WorkspaceCapabilities;
  files: WorkspaceFile[];
}

export interface SectionDetail {
  schema_version: number;
  snapshot_id: string;
  generation: number;
  file_id: string;
  section_id: string;
  generation_id: string;
  language: string;
  source_side: string;
  source: string;
  old_source?: string | null;
  diff?: string | null;
  line_range: LineRange;
  valid_annotation_regions: AnnotationRegion[];
  capabilities: string[];
}

export interface ApiErrorPayload {
  ok: false;
  code: string;
  error: string;
  retryable: boolean;
  snapshot_id?: string;
}

export class SectionApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly retryable: boolean;
  readonly snapshotId?: string;

  constructor(status: number, code: string, message: string, retryable: boolean, snapshotId?: string) {
    super(message);
    this.name = 'SectionApiError';
    this.status = status;
    this.code = code;
    this.retryable = retryable;
    this.snapshotId = snapshotId;
  }
}

export class LruCache<T> {
  private readonly maxEntries: number;
  private readonly cache = new Map<string, T>();

  constructor(maxEntries = 64) {
    this.maxEntries = maxEntries;
  }

  get(key: string): T | undefined {
    const item = this.cache.get(key);
    if (item !== undefined) {
      this.cache.delete(key);
      this.cache.set(key, item);
    }
    return item;
  }

  set(key: string, value: T): void {
    if (this.cache.has(key)) {
      this.cache.delete(key);
    } else if (this.cache.size >= this.maxEntries) {
      const oldestKey = this.cache.keys().next().value;
      if (oldestKey !== undefined) {
        this.cache.delete(oldestKey);
      }
    }
    this.cache.set(key, value);
  }

  has(key: string): boolean {
    return this.cache.has(key);
  }

  clear(): void {
    this.cache.clear();
  }

  size(): number {
    return this.cache.size;
  }
}

export function getWorkspaceIndexPath(mode: WorkspaceMode, sessionId: string | null): string {
  if (mode === 'daemon' && sessionId) {
    return `/api/sessions/${encodeURIComponent(sessionId)}/ui/v1/index`;
  }
  return '/api/ui/v1/index';
}

export function getSectionDetailPath(
  mode: WorkspaceMode,
  sessionId: string | null,
  sectionId: string,
  snapshotId: string
): string {
  const query = `snapshot_id=${encodeURIComponent(snapshotId)}`;
  if (mode === 'daemon' && sessionId) {
    return `/api/sessions/${encodeURIComponent(sessionId)}/ui/v1/sections/${encodeURIComponent(sectionId)}?${query}`;
  }
  return `/api/ui/v1/sections/${encodeURIComponent(sectionId)}?${query}`;
}

export async function fetchWorkspaceIndex(
  mode: WorkspaceMode,
  sessionId: string | null,
  signal?: AbortSignal
): Promise<WorkspaceIndex> {
  const url = getWorkspaceIndexPath(mode, sessionId);
  const response = await fetch(url, { signal });
  if (!response.ok) {
    let message = `Failed to load workspace index (${response.status})`;
    let code = 'index_load_failed';
    try {
      const errJson = (await response.json()) as ApiErrorPayload;
      if (errJson?.error) message = errJson.error;
      if (errJson?.code) code = errJson.code;
    } catch {
      // Non-JSON response
    }
    throw new SectionApiError(response.status, code, message, response.status >= 500);
  }
  return (await response.json()) as WorkspaceIndex;
}

export async function fetchSectionDetail(
  mode: WorkspaceMode,
  sessionId: string | null,
  sectionId: string,
  snapshotId: string,
  signal?: AbortSignal
): Promise<SectionDetail> {
  const url = getSectionDetailPath(mode, sessionId, sectionId, snapshotId);
  const response = await fetch(url, { signal });
  if (!response.ok) {
    let message = `Failed to load section detail (${response.status})`;
    let code = 'section_load_failed';
    let retryable = response.status >= 500;
    let newSnapshotId: string | undefined;

    try {
      const errJson = (await response.json()) as ApiErrorPayload;
      if (errJson?.error) message = errJson.error;
      if (errJson?.code) code = errJson.code;
      if (typeof errJson?.retryable === 'boolean') retryable = errJson.retryable;
      if (errJson?.snapshot_id) newSnapshotId = errJson.snapshot_id;
    } catch {
      // Non-JSON response
    }

    throw new SectionApiError(response.status, code, message, retryable, newSnapshotId);
  }
  return (await response.json()) as SectionDetail;
}
