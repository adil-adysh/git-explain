export type LanguageHelp = 'automatic' | 'explain' | 'brief' | 'skip';
export type Focus = 'balanced' | 'syntax' | 'execution' | 'reasoning' | 'architecture';
export type Detail = 'concise' | 'balanced' | 'detailed';

export interface Preferences {
  language_help?: LanguageHelp | null;
  focus?: Focus | null;
  detail?: Detail | null;
  known_languages?: string[] | null;
  learning_languages?: string[] | null;
  known_frameworks?: string[] | null;
  learning_frameworks?: string[] | null;
  known_domains?: string[] | null;
  learning_domains?: string[] | null;
  known_concepts?: string[] | null;
  learning_concepts?: string[] | null;
}

export interface LearningProfile {
  preferences: Preferences;
  languages: Record<string, Preferences>;
}

export interface LearningConfig {
  defaults: Preferences;
  profile: string | null;
  profiles: Record<string, LearningProfile>;
}

export interface LearningSelection {
  profile?: string | null;
  clear_profile?: boolean;
  use_repository_defaults?: boolean;
  session?: Preferences;
  section?: Preferences;
  overrides?: Preferences;
}

export interface Policy {
  language: string;
  language_help: LanguageHelp;
  focus: Focus;
  detail: Detail;
  known_languages: string[];
  learning_languages: string[];
  known_frameworks: string[];
  learning_frameworks: string[];
  known_domains: string[];
  learning_domains: string[];
  known_concepts: string[];
  learning_concepts: string[];
}

export interface LearningInspection {
  ok: boolean;
  revision: string;
  config: LearningConfig;
  selection: LearningSelection;
  token: string;
  repository_available: boolean;
}

export interface LearningSaved {
  ok: boolean;
  revision: string;
  config: LearningConfig;
}

export interface ResolvedLearning {
  ok: boolean;
  policy_id: string;
  policy: Policy;
  provenance: Record<string, string>;
}

export class LearningApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = 'LearningApiError';
    this.status = status;
  }
}

export function defaultSelection(): LearningSelection {
  return {
    profile: null,
    clear_profile: false,
    use_repository_defaults: false,
    session: {},
    section: {},
    overrides: {},
  };
}

export function learningBase(sessionId: string | null): string {
  if (sessionId && sessionId.trim().length > 0) {
    return `/api/sessions/${encodeURIComponent(sessionId)}/learning`;
  }
  return '/api/learning';
}

export async function inspectLearning(sessionId: string | null): Promise<LearningInspection> {
  const url = learningBase(sessionId);
  const response = await fetch(url, {
    method: 'GET',
    headers: {
      Accept: 'application/json',
    },
  });

  if (!response.ok) {
    let msg = `Failed to inspect learning settings (${response.status})`;
    try {
      const errData = await response.json();
      if (errData && typeof errData.error === 'string') {
        msg = errData.error;
      }
    } catch {
      // ignore json parse errors
    }
    throw new LearningApiError(response.status, msg);
  }

  const data = (await response.json()) as LearningInspection;
  if (!data || data.ok === false) {
    throw new LearningApiError(response.status, 'Invalid response from learning inspection endpoint');
  }
  return data;
}

export async function saveLearning(
  sessionId: string | null,
  revision: string | number,
  config: LearningConfig,
  token: string
): Promise<LearningSaved> {
  const url = learningBase(sessionId);
  const response = await fetch(url, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json',
      'x-git-explain-learning-token': token,
    },
    body: JSON.stringify({
      revision: String(revision),
      config,
    }),
  });

  if (!response.ok) {
    let msg = `Failed to save learning settings (${response.status})`;
    try {
      const errData = await response.json();
      if (errData && typeof errData.error === 'string') {
        msg = errData.error;
      }
    } catch {
      // ignore json parse errors
    }
    throw new LearningApiError(response.status, msg);
  }

  const data = (await response.json()) as LearningSaved;
  if (!data || data.ok === false) {
    throw new LearningApiError(response.status, 'Failed to save learning preferences');
  }
  return data;
}

export async function resolveLearning(
  sessionId: string | null,
  language: string | null,
  selection: LearningSelection | null
): Promise<ResolvedLearning> {
  const url = `${learningBase(sessionId)}/resolve`;
  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json',
    },
    body: JSON.stringify({
      language: language ?? '',
      selection: selection ?? undefined,
    }),
  });

  if (!response.ok) {
    let msg = `Failed to resolve learning policy (${response.status})`;
    try {
      const errData = await response.json();
      if (errData && typeof errData.error === 'string') {
        msg = errData.error;
      }
    } catch {
      // ignore json parse errors
    }
    throw new LearningApiError(response.status, msg);
  }

  const data = (await response.json()) as ResolvedLearning;
  if (!data || data.ok === false) {
    throw new LearningApiError(response.status, 'Failed to resolve learning policy');
  }
  return data;
}
