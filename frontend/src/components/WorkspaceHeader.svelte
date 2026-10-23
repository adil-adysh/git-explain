<script lang="ts">
  import type { RuntimeMetadata } from '../types';

  interface Props {
    metadata?: RuntimeMetadata;
  }

  let { metadata }: Props = $props();
</script>

<header class="workspace-header">
  <div class="header-brand">
    <span class="app-title">Git Explain</span>
    <span class="mode-badge">
      {metadata?.mode === 'daemon' ? 'Daemon session' : 'Standalone'}
    </span>
  </div>
  <div class="header-context">
    {#if metadata?.sessionId}
      <span class="context-item">
        <span class="context-label">Session:</span>
        <span>{metadata.sessionId}</span>
      </span>
    {/if}
    {#if metadata?.repoName}
      <span class="context-item">
        <span class="context-label">Repository:</span>
        <span class="repo-name">{metadata.repoName}</span>
      </span>
    {/if}
    {#if metadata?.revision}
      <span class="context-item">
        <span class="context-label">Revision:</span>
        <span class="revision-name">{metadata.revision}</span>
      </span>
    {/if}
  </div>
</header>
