<script lang="ts">
  import type { WorkspaceFile, WorkspaceSection, SectionDetail } from '../api';
  import type { SessionSettings } from '../types';
  import type { LearningSelection } from '../learning';
  import type { Annotation, Kind } from '../explanations.svelte';
  import ExplanationPane from './ExplanationPane.svelte';

  interface Props {
    disabledReason: string;
    mode: string;
    sessionId: string | null;
    selection: LearningSelection | null;
    view: string | null;
    visible: boolean;
    onSelectionChange: (value: LearningSelection) => void;
    onViewChange: (kind: Kind) => void;
    onStatusChange: (message: string) => void;
    selectedFile: WorkspaceFile | null;
    selectedSection: WorkspaceSection | null;
    detail: SectionDetail | null;
    loading: boolean;
    error: string | null;
    errorRetryable: boolean;
    isOutsideFilter: boolean;
    filteredSections: WorkspaceSection[];
    revisionLabel?: string;
    settings: SessionSettings;
    shouldFocusHeading: boolean;
    onHeadingFocused?: () => void;
    onNavigateSection: (section: WorkspaceSection) => void;
    onRetrySource: () => void;
  }

  let {
    disabledReason,
    mode, sessionId, selection, view, visible, onSelectionChange, onViewChange, onStatusChange,
    selectedFile,
    selectedSection,
    detail,
    loading,
    error,
    errorRetryable,
    isOutsideFilter,
    filteredSections,
    revisionLabel = 'old revision',
    settings,
    shouldFocusHeading,
    onHeadingFocused,
    onNavigateSection,
    onRetrySource,
  }: Props = $props();

  let headingEl = $state<HTMLHeadingElement | null>(null);
  let errorHeadingEl = $state<HTMLHeadingElement | null>(null);
  let activeAnnotations = $state<Annotation[]>([]);
  const sourceLines = $derived(detail?.source.split('\n') ?? []);

  // Focus heading once after explicit navigation including Back
  $effect(() => {
    if (shouldFocusHeading && headingEl && selectedSection && !loading) {
      const sectionId = selectedSection.id;
      const target = error ? errorHeadingEl : headingEl;
      const frame = requestAnimationFrame(() => {
        if (selectedSection?.id !== sectionId) return;
        target?.focus();
        onHeadingFocused?.();
      });
      return () => cancelAnimationFrame(frame);
    }
  });

  // Calculate Previous and Next sections within filtered list of the selected file
  let currentIndex = $derived(
    selectedSection
      ? filteredSections.findIndex((s) => s.id === selectedSection.id)
      : -1
  );

  let prevSection = $derived(
    !isOutsideFilter && currentIndex > 0 ? filteredSections[currentIndex - 1] : null
  );

  let nextSection = $derived(
    !isOutsideFilter && currentIndex >= 0 && currentIndex < filteredSections.length - 1
      ? filteredSections[currentIndex + 1]
      : null
  );

  let prevDisabled = $derived(isOutsideFilter || prevSection === null);
  let nextDisabled = $derived(isOutsideFilter || nextSection === null);

</script>

<article class="reader-container" aria-label="Code section reader">
  {#if !selectedFile}
    <div class="reader-empty-state">
      <h2>Select a file</h2>
      <p>Choose a file from the explorer to view its code sections.</p>
    </div>
  {:else if selectedFile.sections.length === 0}
    <div class="reader-empty-state">
      <h2>No code sections</h2>
      <p>No readable code sections were found in <code>{selectedFile.path_label}</code>. Choose another file.</p>
    </div>
  {:else if !selectedSection}
    <div class="reader-empty-state">
      <h2>Select a code section</h2>
      <p>Choose a code section in <code>{selectedFile.path_label}</code> to view its source.</p>
    </div>
  {:else}
    <header class="reader-header">
      <div class="reader-breadcrumbs">
        <span class="file-breadcrumb">{selectedFile.path_label}</span>
      </div>

      <h2
        tabindex="-1"
        bind:this={headingEl}
        class="reader-heading"
      >
        {selectedFile.path_label} — {selectedSection.name}
      </h2>

      {#if isOutsideFilter}
        <div class="filter-warning-banner" role="note">
          <strong>Not in current filter</strong>
        </div>
      {/if}

      <div class="section-meta-row">
        <span class="meta-badge kind-badge">{selectedSection.kind}</span>
        <span class="meta-badge status-badge status-{selectedSection.change_status}">
          {selectedSection.change_status}
        </span>
        <span class="meta-text">
          Lines {selectedSection.start_line}–{selectedSection.end_line}
        </span>
        {#if !isOutsideFilter && currentIndex >= 0}
          <span class="meta-text position-text">
            Section {currentIndex + 1} of {filteredSections.length}{filteredSections.length !== selectedFile.sections.length ? ` shown (${selectedFile.sections.length} total)` : ''}
          </span>
        {/if}
      </div>

      <nav aria-label="Code section navigation" class="section-nav-controls">
        <button
          type="button"
          class="btn-section-nav"
          disabled={prevDisabled}
          aria-label={prevSection ? `Previous section: ${prevSection.name}` : 'Previous section'}
          onclick={() => prevSection && onNavigateSection(prevSection)}
        >
          Previous section{prevSection ? `: ${prevSection.name}` : ''}
        </button>
        <button
          type="button"
          class="btn-section-nav"
          disabled={nextDisabled}
          aria-label={nextSection ? `Next section: ${nextSection.name}` : 'Next section'}
          onclick={() => nextSection && onNavigateSection(nextSection)}
        >
          Next section{nextSection ? `: ${nextSection.name}` : ''}
        </button>
      </nav>
    </header>

    <div class="reader-body">
      <section aria-labelledby="source-section-heading" class="source-wrapper">
        <h3 id="source-section-heading">Source{selectedSection.source_side === 'before' ? ' — old revision' : ''}</h3>

        {#if loading}
          <div class="loading-box">
            <p>Loading source for {selectedSection.name}...</p>
          </div>
        {:else if error}
          <div class="error-box" role="alert">
            <h3 tabindex="-1" bind:this={errorHeadingEl}>Source could not be loaded</h3>
            <p class="error-text">{error}</p>
            {#if errorRetryable}
              <button type="button" class="btn-retry" onclick={onRetrySource}>
                Retry loading source
              </button>
            {/if}
          </div>
        {:else if detail}
          <div class="source-view" data-source-view>
            {#if settings.plainSource}
              <textarea
                readonly
                class="plain-source-area"
                class:code-wrap={settings.codeWrap}
                aria-label="Source code"
                rows="20"
                value={detail.source}
              ></textarea>
            {:else}
              <pre
                class="code-block"
                class:code-wrap={settings.codeWrap}
              ><code>{#each sourceLines as line, index}<span id={`source-line-${index + 1}`} tabindex="-1" class:annotated-line={activeAnnotations.some(annotation => index + 1 >= annotation.start_line && index + 1 <= annotation.end_line)}>{line}{index < sourceLines.length - 1 ? '\n' : ''}</span>{/each}</code></pre>
            {/if}
          </div>

          {#if detail.diff}
            <details class="source-disclosure diff-disclosure">
              <summary class="disclosure-summary">
                View diff ({revisionLabel})
              </summary>
              <div class="disclosure-content">
                {#if settings.plainSource}
                  <textarea
                    readonly
                    class="plain-source-area diff-text"
                    class:code-wrap={settings.codeWrap}
                    aria-label="Diff"
                    rows="10"
                    value={detail.diff}
                  ></textarea>
                {:else}
                  <pre
                    class="code-block diff-block"
                    class:code-wrap={settings.codeWrap}
                  ><code>{detail.diff}</code></pre>
                {/if}
              </div>
            </details>
          {/if}

          {#if detail.old_source}
            <details class="source-disclosure old-source-disclosure">
              <summary class="disclosure-summary">
                View previous source (before comparison)
              </summary>
              <div class="disclosure-content">
                {#if settings.plainSource}
                  <textarea
                    readonly
                    class="plain-source-area"
                    class:code-wrap={settings.codeWrap}
                    aria-label="Previous source"
                    rows="15"
                    value={detail.old_source}
                  ></textarea>
                {:else}
                  <pre
                    class="code-block"
                    class:code-wrap={settings.codeWrap}
                  ><code>{detail.old_source}</code></pre>
                {/if}
              </div>
            </details>
          {/if}
        {/if}
      </section>

      {#if detail && !loading && !error}
        <ExplanationPane {detail} {mode} {sessionId} {selection} view={view} {visible} {disabledReason} hideExplanations={settings.hideExplanations}
          {onSelectionChange} {onViewChange} {onStatusChange} onAnnotationsChange={(value) => { activeAnnotations = value; }} />
      {/if}
    </div>
  {/if}
</article>
