<script lang="ts">
  import { onMount } from 'svelte';
  import type {
    Destination,
    RouteState,
    ExploreSelection,
    SessionSettings,
    RuntimeMetadata,
    PersonalizationLoader,
  } from './types';
  import type { WorkspaceIndex } from './api';
  import { parseRoute, focusDestinationHeading } from './navigation';
  import { defaultSettings, applySettings } from './theme';
  import WorkspaceHeader from './components/WorkspaceHeader.svelte';
  import AppNavigation from './components/AppNavigation.svelte';
  import ExploreWorkspace from './components/ExploreWorkspace.svelte';
  import SettingsPage from './components/SettingsPage.svelte';
  import HelpPage from './components/HelpPage.svelte';
  import StatusRegion from './components/StatusRegion.svelte';
  import type { LearningSelection } from './learning';
  import { explanations, type Kind } from './explanations.svelte';
  import PersonalizationPage from './components/PersonalizationPage.svelte';

  interface Props {
    metadata?: RuntimeMetadata;
    statusMessage?: string;
    loadPersonalization?: PersonalizationLoader;
  }

  let {
    metadata: initialMetadata = { mode: 'standalone' },
    statusMessage = '',
    loadPersonalization,
  }: Props = $props();

  // Session settings state (in session memory only)
  let settings = $state<SessionSettings>({ ...defaultSettings });

  // Loaded metadata from backend index
  let loadedMetadata = $state<Partial<RuntimeMetadata>>({});

  // Runtime metadata merged with actual backend revision/session info
  let currentMetadata = $derived<RuntimeMetadata>({
    ...initialMetadata,
    ...loadedMetadata,
  });

  // Route state
  let route = $state<RouteState>(parseRoute(window.location.pathname, window.location.search));

  // Remembered Explore query selection (file, section, view)
  let exploreSelection = $state<ExploreSelection>({
    file: null,
    section: null,
    view: null,
  });

  // Track status message
  let activeStatus = $state('');
  let learningSelection = $state<LearningSelection | null>(null);
  let personalizationVisited = $state(false);
  $effect(() => { if (route.destination === 'personalization') personalizationVisited = true; });

  function handleViewChange(kind: Kind) {
    const url = new URL(window.location.href);
    url.searchParams.set('view', kind);
    window.history.replaceState(null, '', url);
    updateRouteFromLocation();
  }

  $effect(() => {
    if (statusMessage) {
      activeStatus = statusMessage;
    }
  });

  function updateRouteFromLocation(): void {
    if (typeof window === 'undefined') return;
    const parsed = parseRoute(window.location.pathname, window.location.search);
    route = parsed;

    if (parsed.destination === 'explore') {
      exploreSelection = {
        file: parsed.file,
        section: parsed.section,
        view: parsed.view,
      };
    }
  }

  function handleNavigate(destination: Destination, url: string): void {
    if (typeof window === 'undefined') return;
    window.history.pushState(null, '', url);
    updateRouteFromLocation();
    focusDestinationHeading();

    if (destination === 'personalization' && loadPersonalization) {
      void loadPersonalization().catch(() => undefined);
    }
  }

  function handleNavigateExplore(_destination: 'explore', url: string): void {
    if (typeof window === 'undefined') return;
    window.history.pushState(null, '', url);
    updateRouteFromLocation();
  }

  function handleExploreSelectionChange(file: string | null, section: string | null): void {
    exploreSelection = {
      file,
      section,
      view: exploreSelection.view,
    };
  }

  function handlePopState(): void {
    if (typeof window === 'undefined') return;
    const parsed = parseRoute(window.location.pathname, window.location.search);
    route = parsed;

    if (parsed.destination === 'explore') {
      exploreSelection = {
        file: parsed.file,
        section: parsed.section,
        view: parsed.view,
      };
      // For explore destination with a section, section reader heading will focus itself
      if (!parsed.section) {
        focusDestinationHeading();
      }
    } else {
      focusDestinationHeading();
    }
  }

  function handleUpdateSettings(newSettings: Partial<SessionSettings>): void {
    settings = { ...settings, ...newSettings };
    applySettings(settings);
  }

  function handleMetadataLoaded(index: WorkspaceIndex): void {
    loadedMetadata = {
      mode: route.mode,
      sessionId: index.session_id ?? route.sessionId ?? null,
      revision: index.revision_label,
      version: index.runtime?.version,
      repoName: index.repository_label,
      model: index.runtime?.model,
    };
  }

  onMount(() => {
    updateRouteFromLocation();
    applySettings(settings);

    window.addEventListener('popstate', handlePopState);
    return () => {
      window.removeEventListener('popstate', handlePopState);
    };
  });
</script>

<div class="app-container">
  <div class="skip-links">
    <a href="#main" class="skip-link">Skip to main content</a>
    <a href="#explorer" class="skip-link">Skip to explorer</a>
  </div>

  <WorkspaceHeader metadata={currentMetadata} />

  <AppNavigation
    basePath={route.basePath}
    currentDestination={route.destination}
    {exploreSelection}
    onNavigate={handleNavigate}
  />

  <main id="main" tabindex="-1">
    <div
      class="explore-view-container"
      hidden={route.destination !== 'explore'}
      inert={route.destination !== 'explore'}
      style:display={route.destination === 'explore' ? undefined : 'none'}
    >
      <ExploreWorkspace
        selection={learningSelection} onLearningSelectionChange={value => { learningSelection = value; }} onViewChange={handleViewChange}
        metadata={currentMetadata}
        {route}
        {settings}
        onStatusChange={(msg) => {
          activeStatus = msg;
        }}
        onMetadataLoaded={handleMetadataLoaded}
        onNavigate={handleNavigateExplore}
        onSelectionChange={handleExploreSelectionChange}
      />
    </div>

    {#if personalizationVisited}
      <div hidden={route.destination !== 'personalization'} inert={route.destination !== 'personalization'}>
        <PersonalizationPage sessionId={route.sessionId} selection={learningSelection} onSelectionChange={value => { learningSelection = value; }} onStatusChange={message => { activeStatus = message; }} onSaved={() => explanations.invalidatePolicies()} />
      </div>
    {/if}
    {#if route.destination === 'settings'}
      <SettingsPage
        metadata={currentMetadata}
        {settings}
        onUpdateSettings={handleUpdateSettings}
      />
    {:else if route.destination === 'help'}
      <HelpPage />
    {/if}
  </main>

  <StatusRegion message={activeStatus} />
</div>
