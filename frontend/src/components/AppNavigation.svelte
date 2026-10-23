<script lang="ts">
  import type { Destination, ExploreSelection } from '../types';
  import { buildRouteUrl } from '../navigation';

  interface Props {
    basePath: string;
    currentDestination: Destination;
    exploreSelection: ExploreSelection;
    onNavigate: (destination: Destination, url: string) => void;
  }

  let { basePath, currentDestination, exploreSelection, onNavigate }: Props = $props();

  let exploreUrl = $derived(buildRouteUrl(basePath, 'explore', exploreSelection));
  let personalizationUrl = $derived(buildRouteUrl(basePath, 'personalization'));
  let settingsUrl = $derived(buildRouteUrl(basePath, 'settings'));
  let helpUrl = $derived(buildRouteUrl(basePath, 'help'));

  function onLinkClick(event: MouseEvent, destination: Destination, url: string): void {
    if (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) {
      return;
    }
    event.preventDefault();
    onNavigate(destination, url);
  }
</script>

<nav aria-label="Application" class="app-navigation">
  <ul class="nav-list">
    <li>
      <a
        href={exploreUrl}
        class="nav-link"
        aria-current={currentDestination === 'explore' ? 'page' : undefined}
        onclick={(e) => onLinkClick(e, 'explore', exploreUrl)}
      >
        Explore
      </a>
    </li>
    <li>
      <a
        href={personalizationUrl}
        class="nav-link"
        aria-current={currentDestination === 'personalization' ? 'page' : undefined}
        onclick={(e) => onLinkClick(e, 'personalization', personalizationUrl)}
      >
        Personalization
      </a>
    </li>
    <li>
      <a
        href={settingsUrl}
        class="nav-link"
        aria-current={currentDestination === 'settings' ? 'page' : undefined}
        onclick={(e) => onLinkClick(e, 'settings', settingsUrl)}
      >
        Settings
      </a>
    </li>
    <li>
      <a
        href={helpUrl}
        class="nav-link"
        aria-current={currentDestination === 'help' ? 'page' : undefined}
        onclick={(e) => onLinkClick(e, 'help', helpUrl)}
      >
        Help
      </a>
    </li>
  </ul>
</nav>
