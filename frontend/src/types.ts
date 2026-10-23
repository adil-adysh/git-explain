export type Destination = 'explore' | 'personalization' | 'settings' | 'help';

export type WorkspaceMode = 'standalone' | 'daemon';

export interface RouteState {
  mode: WorkspaceMode;
  sessionId: string | null;
  basePath: string;
  destination: Destination;
  file: string | null;
  section: string | null;
  view: string | null;
}

export interface ExploreSelection {
  file: string | null;
  section: string | null;
  view: string | null;
}

export type Theme = 'system' | 'light' | 'dark';
export type FontScale = 'normal' | 'large' | 'x-large';

export interface SessionSettings {
  theme: Theme;
  fontScale: FontScale;
  codeWrap: boolean;
  plainSource: boolean;
  hideExplanations: boolean;
}

export interface RuntimeMetadata {
  mode: WorkspaceMode;
  sessionId?: string | null;
  repoName?: string;
  revision?: string;
  version?: string;
  model?: string;
}

export type PersonalizationLoader = () => Promise<void>;
