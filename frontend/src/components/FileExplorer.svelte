<script lang="ts">
  import type { WorkspaceFile, WorkspaceSection } from '../api';
  import { buildRouteUrl } from '../navigation';

  interface Props {
    basePath: string;
    files: WorkspaceFile[];
    showChangeCounts: boolean;
    selectedFileId: string | null;
    selectedSectionId: string | null;
    filterText: string;
    statusFilter: 'all' | 'changed' | 'unchanged';
    filteredSections: WorkspaceSection[];
    onSelectFile: (file: WorkspaceFile) => void;
    onSelectSection: (section: WorkspaceSection, file: WorkspaceFile) => void;
    onFilterTextChange: (text: string) => void;
    onStatusFilterChange: (status: 'all' | 'changed' | 'unchanged') => void;
    onClearFilter: () => void;
  }

  let {
    basePath,
    files,
    showChangeCounts,
    selectedFileId,
    selectedSectionId,
    filterText,
    statusFilter,
    filteredSections,
    onSelectFile,
    onSelectSection,
    onFilterTextChange,
    onStatusFilterChange,
    onClearFilter,
  }: Props = $props();

  let selectedFile = $derived(files.find((f) => f.id === selectedFileId) ?? null);
  let fileFilter = $state('');
  const visibleFiles = $derived(files.filter(file => file.path_label.toLowerCase().includes(fileFilter.trim().toLowerCase()) || file.sections.some(section => section.name.toLowerCase().includes(fileFilter.trim().toLowerCase()))));

  function getFileUrl(file: WorkspaceFile): string {
    return buildRouteUrl(basePath, 'explore', {
      file: file.id,
      section: file.sections.find(section => !showChangeCounts || section.change_status !== 'unchanged')?.id ?? null,
      view: null,
    });
  }

  function getSectionUrl(file: WorkspaceFile, section: WorkspaceSection): string {
    return buildRouteUrl(basePath, 'explore', {
      file: file.id,
      section: section.id,
      view: null,
    });
  }

  function handleFileClick(event: MouseEvent, file: WorkspaceFile): void {
    if (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) {
      return;
    }
    event.preventDefault();
    onSelectFile(file);
  }

  function handleSectionClick(
    event: MouseEvent,
    section: WorkspaceSection,
    file: WorkspaceFile
  ): void {
    if (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) {
      return;
    }
    event.preventDefault();
    onSelectSection(section, file);
  }

  function handleInput(event: Event): void {
    const target = event.target as HTMLInputElement;
    onFilterTextChange(target.value);
  }
</script>

<nav id="explorer" aria-label="Files and code sections" class="explorer-nav">
  <div class="explorer-files-section">
    <h2 class="explorer-heading">Files ({visibleFiles.length} of {files.length})</h2>
    <p>Only supported source files are listed.</p>
    <label for="file-filter">Filter files</label><input id="file-filter" type="search" bind:value={fileFilter} />
    {#if fileFilter}<button onclick={() => { fileFilter = ''; }}>Clear file filter</button>{/if}
    {#if files.length === 0}
      <p class="empty-state-message">No supported files found.</p>
    {:else if visibleFiles.length === 0}<p>No files match the current filter.</p>
    {:else}
      <ul class="file-list">
        {#each visibleFiles as file (file.id)}
          {@const changedCount = file.sections.filter(section => section.change_status !== 'unchanged').length}
          <li class="file-item">
            <a
              href={getFileUrl(file)}
              class="file-link"
              class:selected={file.id === selectedFileId}
              aria-current={file.id === selectedFileId ? 'true' : undefined}
              onclick={(e) => handleFileClick(e, file)}
            >
              <span class="file-path">{file.path_label}</span>
              <span class="file-status badge badge-{file.status}">{file.status}</span>
              <span class="file-count">
                {file.section_count} {file.section_count === 1 ? 'section' : 'sections'}
                {#if showChangeCounts} · {changedCount} changed · {file.sections.length - changedCount} unchanged{/if}
              </span>
            </a>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#if selectedFile}
    <div class="explorer-sections-section">
      <h2 class="explorer-heading">Code sections in {selectedFile.path_label}</h2>
      <p class="section-count">Showing {filteredSections.length} of {selectedFile.sections.length} code sections</p>

      <div class="section-filter-container">
        <div class="filter-input-row">
          <label for="section-filter-input" class="filter-label">Filter sections</label>
          <input
            id="section-filter-input"
            type="text"
            class="filter-input"
            value={filterText}
            oninput={handleInput}
            placeholder="Filter sections..."
          />
          <button
            type="button"
            class="btn-clear-filter"
            onclick={onClearFilter}
          >
            Clear section filter
          </button>
        </div>

        <div class="status-filter-group" role="group" aria-label="Filter by change status">
          <button
            type="button"
            class="btn-status-filter"
            class:active={statusFilter === 'all'}
            aria-pressed={statusFilter === 'all'}
            onclick={() => onStatusFilterChange('all')}
          >
            All
          </button>
          <button
            type="button"
            class="btn-status-filter"
            class:active={statusFilter === 'changed'}
            aria-pressed={statusFilter === 'changed'}
            onclick={() => onStatusFilterChange('changed')}
          >
            Changed
          </button>
          <button
            type="button"
            class="btn-status-filter"
            class:active={statusFilter === 'unchanged'}
            aria-pressed={statusFilter === 'unchanged'}
            onclick={() => onStatusFilterChange('unchanged')}
          >
            Unchanged
          </button>
        </div>
      </div>

      {#if selectedFile.sections.length === 0}
        <p class="empty-state-message">No code sections discovered in this file.</p>
      {:else if filteredSections.length === 0}
        <p class="empty-state-message">No code sections match the current filter.</p>
      {:else}
        <ol class="section-list">
          {#each filteredSections as section (section.id)}
            <li class="section-item">
              <a
                href={getSectionUrl(selectedFile, section)}
                class="section-link"
                class:selected={section.id === selectedSectionId}
                aria-current={section.id === selectedSectionId ? 'true' : undefined}
                onclick={(e) => handleSectionClick(e, section, selectedFile!)}
              >
                <span class="section-name">{section.name}</span>
                <span class="section-kind">{section.kind}</span>
                <span class="section-range">lines {section.start_line}–{section.end_line}</span>
                <span class="section-status badge badge-{section.change_status}">
                  {section.change_status}
                </span>
              </a>
            </li>
          {/each}
        </ol>
      {/if}
    </div>
  {/if}
</nav>
