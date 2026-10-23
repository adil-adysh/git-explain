import type { Destination, RouteState, ExploreSelection, WorkspaceMode } from './types';

const VALID_DESTINATIONS: readonly Destination[] = ['explore', 'personalization', 'settings', 'help'] as const;

export function isDestination(value: string | null): value is Destination {
  return value !== null && (VALID_DESTINATIONS as readonly string[]).includes(value);
}

export function parseRoute(pathname: string, search: string): RouteState {
  const daemonMatch = pathname.match(/^\/sessions\/([^/]+)(?:\/workspace(?:\/.*)?)?$/);
  let mode: WorkspaceMode = 'standalone';
  let sessionId: string | null = null;
  let basePath = '/workspace';

  if (daemonMatch) {
    mode = 'daemon';
    sessionId = decodeURIComponent(daemonMatch[1]);
    basePath = `/sessions/${encodeURIComponent(sessionId)}/workspace`;
  } else {
    basePath = '/workspace';
  }

  const params = new URLSearchParams(search);
  const destParam = params.get('destination');
  const destination: Destination = isDestination(destParam) ? destParam : 'explore';
  const file = params.get('file');
  const section = params.get('section');
  const view = params.get('view');

  return {
    mode,
    sessionId,
    basePath,
    destination,
    file,
    section,
    view,
  };
}

export function buildRouteUrl(
  basePath: string,
  destination: Destination,
  selection?: ExploreSelection | null
): string {
  const params = new URLSearchParams();
  params.set('destination', destination);

  if (destination === 'explore' && selection) {
    if (selection.file) params.set('file', selection.file);
    if (selection.section) params.set('section', selection.section);
    if (selection.view) params.set('view', selection.view);
  }

  const query = params.toString();
  return query ? `${basePath}?${query}` : basePath;
}

export function focusDestinationHeading(): void {
  if (typeof window === 'undefined') return;
  requestAnimationFrame(() => {
    const heading = [...document.querySelectorAll('main h1')].find(element => element.checkVisibility());
    if (heading instanceof HTMLElement) {
      heading.focus();
    }
  });
}
