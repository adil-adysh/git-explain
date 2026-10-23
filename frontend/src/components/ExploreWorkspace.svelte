<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type {
    WorkspaceIndex,
    WorkspaceFile,
    WorkspaceSection,
    SectionDetail,
    SectionApiError,
  } from '../api';
  import { fetchWorkspaceIndex, fetchSectionDetail, LruCache } from '../api';
  import type { RouteState, SessionSettings, RuntimeMetadata } from '../types';
  import { buildRouteUrl } from '../navigation';
  import type { LearningSelection } from '../learning';
  import type { Kind } from '../explanations.svelte';
  import FileExplorer from './FileExplorer.svelte';
  import SectionReader from './SectionReader.svelte';

  interface Props {
    selection: LearningSelection | null;
    onLearningSelectionChange: (value: LearningSelection) => void;
    onViewChange: (kind: Kind) => void;
    metadata?: RuntimeMetadata;
    route: RouteState;
    settings: SessionSettings;
    onStatusChange: (message: string) => void;
    onMetadataLoaded: (index: WorkspaceIndex) => void;
    onNavigate: (destination: 'explore', url: string) => void;
    onSelectionChange: (file: string | null, section: string | null) => void;
  }

  let {
    selection, onLearningSelectionChange, onViewChange,
    metadata: _metadata,
    route,
    settings,
    onStatusChange,
    onMetadataLoaded,
    onNavigate,
    onSelectionChange,
  }: Props = $props();

  // In-memory LRU cache for section details (max 64 entries)
  const sourceCache = new LruCache<SectionDetail>(64);
  const lastReadSections = new Map<string, string>();

  // Workspace index state
  let workspaceIndex = $state<WorkspaceIndex | null>(null);
  let indexLoading = $state(true);
  let indexError = $state<string | null>(null);
  let indexRetryable = $state(true);

  // Selection state
  let selectedFileId = $state<string | null>(null);
  let selectedSectionId = $state<string | null>(null);

  // Filter state
  let filterText = $state('');
  let statusFilter = $state<'all' | 'changed' | 'unchanged'>('all');

  // Reader source detail state
  let detail = $state<SectionDetail | null>(null);
  let detailLoading = $state(false);
  let detailError = $state<string | null>(null);
  let detailRetryable = $state(false);
  let selectionNotice = $state('');
  let stale = $state(false);
  let sessionGone = $state(false);

  // Focus trigger token for Reader h2
  let shouldFocusReaderHeading = $state(false);

  // Request sequence and abort tracking
  let activeAbortController: AbortController | null = null;
  let requestSequence = 0;

  // Derived selected file and section
  let selectedFile = $derived(
    workspaceIndex?.files.find((f) => f.id === selectedFileId) ?? null
  );

  let selectedSection = $derived(
    selectedFile?.sections.find((s) => s.id === selectedSectionId) ?? null
  );

  // Filtered sections within current file
  let filteredSections = $derived.by<WorkspaceSection[]>(() => {
    if (!selectedFile) return [];
    let list = selectedFile.sections;

    if (statusFilter === 'changed') {
      list = list.filter((s) => s.change_status !== 'unchanged');
    } else if (statusFilter === 'unchanged') {
      list = list.filter((s) => s.change_status === 'unchanged');
    }

    const trimmed = filterText.trim().toLowerCase();
    if (trimmed) {
      list = list.filter(
        (s) =>
          s.name.toLowerCase().includes(trimmed) ||
          s.kind.toLowerCase().includes(trimmed) ||
          s.change_status.toLowerCase().includes(trimmed)
      );
    }

    return list;
  });

  // Whether the selected section is outside the current filter
  let isOutsideFilter = $derived(
    selectedSectionId !== null &&
      !filteredSections.some((s) => s.id === selectedSectionId)
  );

  async function loadIndex(): Promise<void> {
    activeAbortController?.abort();
    requestSequence++;
    sourceCache.clear();
    lastReadSections.clear();
    indexLoading = true;
    indexError = null;
    try {
      const index = await fetchWorkspaceIndex(route.mode, route.sessionId);
      workspaceIndex = index;
      stale = false;
      sessionGone = false;
      indexLoading = false;
      onMetadataLoaded(index);

      // Check if route has file / section to restore
      applyRouteSelection(route, index, false);
    } catch (err: unknown) {
      indexLoading = false;
      const apiErr = err as SectionApiError;
      indexError = apiErr?.message || 'Failed to load workspace index';
      indexRetryable = apiErr?.retryable ?? true;
      onStatusChange(`Error: ${indexError}`);
    }
  }

  async function reloadFiles() {
    await loadIndex();
    await tick();
    document.getElementById('explore-heading')?.focus();
  }

  function applyRouteSelection(
    currentRoute: RouteState,
    index: WorkspaceIndex,
    isExplicitNav: boolean
  ): void {
    activeAbortController?.abort();
    requestSequence++;
    detailLoading = false;
    detailError = null;
    shouldFocusReaderHeading = false;
    selectionNotice = '';
    if (!currentRoute.file) {
      selectedFileId = null;
      selectedSectionId = null;
      detail = null;
      return;
    }

    const file = index.files.find((f) => f.id === currentRoute.file);
    if (!file) {
      selectionNotice = 'This file is unavailable. Choose a file from the current list.';
      selectedFileId = null;
      selectedSectionId = null;
      detail = null;
      return;
    }

    selectedFileId = file.id;

    if (!currentRoute.section) {
      selectedSectionId = null;
      detail = null;
      return;
    }

    const section = file.sections.find((s) => s.id === currentRoute.section);
    if (!section) {
      selectionNotice = 'This code section is unavailable. Choose a section from the current list.';
      selectedSectionId = null;
      detail = null;
      return;
    }

    selectedSectionId = section.id;
    lastReadSections.set(file.id, section.id);
    void loadSectionSource(file, section, index.snapshot_id, isExplicitNav);
  }

  async function loadSectionSource(
    file: WorkspaceFile,
    section: WorkspaceSection,
    snapshotId: string,
    focusHeading = false
  ): Promise<void> {
    activeAbortController?.abort();
    const currentSeq = ++requestSequence;
    const cacheKey = `${snapshotId}:${section.id}`;
    const cached = sourceCache.get(cacheKey);

    if (cached) {
      detail = cached;
      detailLoading = false;
      detailError = null;
      shouldFocusReaderHeading = focusHeading;
      return;
    }

    if (activeAbortController) {
      activeAbortController.abort();
    }
    const controller = new AbortController();
    activeAbortController = controller;
    detail = null;
    detailLoading = true;
    detailError = null;
    detailRetryable = false;

    try {
      const result = await fetchSectionDetail(
        route.mode,
        route.sessionId,
        section.id,
        snapshotId,
        controller.signal
      );

      // Verify sequence and identity before updating view
      if (currentSeq !== requestSequence) return;
      if (
        result.snapshot_id !== snapshotId ||
        result.section_id !== section.id ||
        result.file_id !== file.id
      ) {
        throw new Error('The code changed while loading. Reload the file list before continuing.');
      }

      if (result.source.length + (result.old_source?.length ?? 0) + (result.diff?.length ?? 0) < 2 * 1024 * 1024) sourceCache.set(cacheKey, result);

      if (selectedSectionId === section.id && selectedFileId === file.id) {
        detail = result;
        detailLoading = false;
        shouldFocusReaderHeading = focusHeading;
      }
    } catch (err: unknown) {
      if (currentSeq !== requestSequence) return;
      if (err instanceof Error && err.name === 'AbortError') return;

      const apiErr = err as SectionApiError;
      detailLoading = false;
      detailError = apiErr?.message || 'Failed to load code section source';
      detailRetryable = apiErr?.retryable ?? true;
      shouldFocusReaderHeading = focusHeading;
      onStatusChange(`Error: ${detailError}`);
    }
  }

  function handleSelectFile(file: WorkspaceFile): void {
    selectedFileId = file.id;
    activeAbortController?.abort();
    requestSequence++;
    selectedSectionId = null;
    detail = null;
    detailLoading = false;
    detailError = null;
    selectionNotice = '';
    filterText = '';
    statusFilter = 'all';

    const remembered = lastReadSections.get(file.id);
    const section = file.sections.find(section => section.id === remembered) ?? file.sections[0];
    if (section) { handleSelectSection(section, file); return; }

    const url = buildRouteUrl(route.basePath, 'explore', {
      file: file.id,
      section: null,
      view: null,
    });
    onSelectionChange(file.id, null);
    onNavigate('explore', url);
  }

  function handleSelectSection(section: WorkspaceSection, file: WorkspaceFile): void {
    selectedFileId = file.id;
    selectedSectionId = section.id;
    lastReadSections.set(file.id, section.id);

    const url = buildRouteUrl(route.basePath, 'explore', {
      file: file.id,
      section: section.id,
      view: null,
    });
    onSelectionChange(file.id, section.id);
    onNavigate('explore', url);

    if (workspaceIndex) {
      void loadSectionSource(file, section, workspaceIndex.snapshot_id, true);
    }
  }

  function handleNavigateSection(section: WorkspaceSection): void {
    if (!selectedFile) return;
    handleSelectSection(section, selectedFile);
  }

  function handleClearFilter(): void {
    filterText = '';
    statusFilter = 'all';
  }

  function handleRetrySource(): void {
    if (selectedFile && selectedSection && workspaceIndex) {
      void loadSectionSource(selectedFile, selectedSection, workspaceIndex.snapshot_id, true);
    }
  }

  // React to route updates (e.g. popstate back/forward navigation)
  $effect(() => {
    if (workspaceIndex && route.destination === 'explore') {
      const isFileDiff = route.file !== selectedFileId && route.file !== selectedFile?.path_label;
      const isSecDiff = route.section !== selectedSectionId && route.section !== selectedSection?.name;

      if (isFileDiff || isSecDiff) {
        applyRouteSelection(route, workspaceIndex, true);
      }
    }
  });

  onMount(() => {
    void loadIndex();
    let polling = false;
    const poll = setInterval(async () => {
      if (route.mode !== 'daemon' || !route.sessionId || !workspaceIndex || polling || sessionGone) return;
      polling = true;
      try {
        const response = await fetch(`/api/sessions/${encodeURIComponent(route.sessionId)}/ui/v1/workspace`);
        if (response.status === 410) { sessionGone = true; stale = true; onStatusChange('This repository session expired. Reopen it with git explain diff.'); return; }
        if (!response.ok) return;
        const current = await response.json();
        if (current.snapshot_id !== workspaceIndex.snapshot_id && !stale) {
          stale = true;
          onStatusChange('The Git comparison changed. Reload files to inspect current code.');
        }
      } catch { /* Source remains readable during a temporary connection failure. */ }
      finally { polling = false; }
    }, 5000);
    return () => { clearInterval(poll); activeAbortController?.abort(); };
  });
</script>

<div class="explore-workspace">
  <h1 id="explore-heading" tabindex="-1">Explore</h1>
  {#if selectionNotice}<p>{selectionNotice}</p>{/if}
  {#if stale}
    <div class="workspace-error-card"><p>{sessionGone ? 'This repository session expired. Reopen it with git explain diff.' : 'The Git comparison changed. Reload files to inspect current code.'}</p>
      {#if !sessionGone}<button onclick={reloadFiles}>Reload files</button>{/if}
    </div>
  {/if}

  {#if indexLoading}
    <div class="workspace-status-card">
      <p>Loading repository files and code sections...</p>
    </div>
  {:else if indexError}
    <div class="workspace-error-card" role="alert">
      <h2>Failed to load workspace</h2>
      <p>{indexError}</p>
      {#if indexRetryable}
        <button type="button" class="btn-retry" onclick={loadIndex}>
          Retry loading files
        </button>
      {/if}
    </div>
  {:else if workspaceIndex}
    <div class="explore-panels-layout">
      <FileExplorer
        basePath={route.basePath}
        files={workspaceIndex.files}
        {selectedFileId}
        {selectedSectionId}
        {filterText}
        {statusFilter}
        {filteredSections}
        onSelectFile={handleSelectFile}
        onSelectSection={handleSelectSection}
        onFilterTextChange={(t) => (filterText = t)}
        onStatusFilterChange={(s) => (statusFilter = s)}
        onClearFilter={handleClearFilter}
      />

      <SectionReader
        disabledReason={stale ? 'Reload files before generating explanations for the changed comparison.' : ''}
        mode={workspaceIndex.mode} sessionId={route.sessionId} {selection} view={route.view} visible={route.destination === 'explore'}
        onSelectionChange={onLearningSelectionChange} {onViewChange} {onStatusChange}
        {selectedFile}
        {selectedSection}
        {detail}
        loading={detailLoading}
        error={detailError}
        errorRetryable={detailRetryable}
        {isOutsideFilter}
        {filteredSections}
        revisionLabel={workspaceIndex.revision_label}
        {settings}
        shouldFocusHeading={shouldFocusReaderHeading}
        onHeadingFocused={() => {
          shouldFocusReaderHeading = false;
        }}
        onNavigateSection={handleNavigateSection}
        onRetrySource={handleRetrySource}
      />
    </div>
  {/if}
</div>
