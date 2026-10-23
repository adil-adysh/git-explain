<script lang="ts">
  import type { RuntimeMetadata, SessionSettings, Theme, FontScale } from '../types';

  interface Props {
    metadata?: RuntimeMetadata;
    settings: SessionSettings;
    onUpdateSettings: (newSettings: Partial<SessionSettings>) => void;
  }

  let { metadata, settings, onUpdateSettings }: Props = $props();

  function onThemeChange(e: Event) {
    const target = e.target as HTMLSelectElement;
    onUpdateSettings({ theme: target.value as Theme });
  }

  function onFontScaleChange(e: Event) {
    const target = e.target as HTMLSelectElement;
    onUpdateSettings({ fontScale: target.value as FontScale });
  }

  function onCodeWrapChange(e: Event) {
    const target = e.target as HTMLInputElement;
    onUpdateSettings({ codeWrap: target.checked });
  }

  function onPlainSourceChange(e: Event) {
    const target = e.target as HTMLInputElement;
    onUpdateSettings({ plainSource: target.checked });
  }
</script>

<section aria-labelledby="destination-heading" class="page-container">
  <h1 id="destination-heading" tabindex="-1">Settings</h1>
  <p class="page-description">
    Configure appearance, accessibility, and review runtime metadata. Settings are stored in session memory only and will reset when closed.
  </p>

  <div class="settings-section">
    <h2>Appearance</h2>
    <div class="form-group">
      <label for="setting-theme">Color Theme</label>
      <select id="setting-theme" value={settings.theme} onchange={onThemeChange}>
        <option value="system">System Default</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
      <p class="help-text">Select system default to follow your operating system appearance preference.</p>
    </div>

    <div class="form-group">
      <label for="setting-font-scale">Font Scale</label>
      <select id="setting-font-scale" value={settings.fontScale} onchange={onFontScaleChange}>
        <option value="normal">Normal (100%)</option>
        <option value="large">Large (115%)</option>
        <option value="x-large">Extra Large (130%)</option>
      </select>
      <p class="help-text">Adjusts text size across the workspace interface.</p>
    </div>

    <div class="form-group">
      <label class="checkbox-label" for="setting-code-wrap">
        <input
          id="setting-code-wrap"
          type="checkbox"
          checked={settings.codeWrap}
          onchange={onCodeWrapChange}
        />
        <span>Wrap long code lines</span>
      </label>
      <p class="help-text">When disabled, code blocks scroll horizontally without wrapping lines.</p>
    </div>
  </div>

  <div class="settings-section">
    <h2>Accessibility &amp; Reading</h2>
    <div class="form-group">
      <label class="checkbox-label" for="setting-plain-source">
        <input
          id="setting-plain-source"
          type="checkbox"
          checked={settings.plainSource}
          onchange={onPlainSourceChange}
        />
        <span>Read source as plain text</span>
      </label>
      <p class="help-text">
        Disables complex code styling and presents plain text, recommended for some screen readers.
      </p>
    </div>
  </div>

  <div class="settings-section">
    <h2>Runtime Information</h2>
    <p>Model: {metadata?.model ?? 'Not loaded yet'}. Use <code>git explain profile list</code> and <code>git explain profile use &lt;name&gt;</code> to change the model profile.</p>
    <p class="help-text">
      Read-only metadata provided by the active Git Explain session. Model endpoints and credentials are managed exclusively via the CLI and configuration files.
    </p>

    <div class="meta-grid">
      <div class="meta-item">
        <div class="meta-key">Execution Mode</div>
        <div class="meta-value">{metadata?.mode === 'daemon' ? 'Daemon Session' : 'Standalone'}</div>
      </div>
      <div class="meta-item">
        <div class="meta-key">Session ID</div>
        <div class="meta-value">{metadata?.sessionId || 'None (standalone)'}</div>
      </div>
      <div class="meta-item">
        <div class="meta-key">Repository</div>
        <div class="meta-value">{metadata?.repoName || 'Not loaded yet'}</div>
      </div>
      <div class="meta-item">
        <div class="meta-key">Revision / Target</div>
        <div class="meta-value">{metadata?.revision || 'Not loaded yet'}</div>
      </div>
      <div class="meta-item">
        <div class="meta-key">Version</div>
        <div class="meta-value">{metadata?.version || 'Development build'}</div>
      </div>
    </div>
  </div>

  <div class="settings-section">
    <h2>Explanations</h2>
    <label for="setting-hide-explanations"><input id="setting-hide-explanations" type="checkbox" checked={settings.hideExplanations} onchange={event => onUpdateSettings({ hideExplanations: event.currentTarget.checked })} /> Hide explanations and annotations</label>
    <p>Generated results are retained while hidden.</p>
  </div>
</section>
