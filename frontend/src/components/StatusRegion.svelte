<script lang="ts">
  interface Props {
    message?: string;
  }

  let { message = '' }: Props = $props();
</script>

<div id="app-status" role="status" aria-live="polite" aria-atomic="true" class="status-region">
  {#if message}
    <p>{message}</p>
  {/if}
</div>
