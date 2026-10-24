<script lang="ts">
  import { onMount } from 'svelte';
  import {
    type LearningConfig,
    type LearningSelection,
    type Preferences,
    type LearningInspection,
    defaultSelection,
    inspectLearning,
    saveLearning,
    LearningApiError,
  } from '../learning';
  import PreferencesFields from './PreferencesFields.svelte';

  interface Props {
    onSaved?: () => void;
    sessionId: string | null;
    selection: LearningSelection | null;
    onSelectionChange: (value: LearningSelection) => void;
    onStatusChange: (message: string) => void;
  }

  let {
    onSaved,
    sessionId,
    selection,
    onSelectionChange,
    onStatusChange,
  }: Props = $props();

  let loading = $state<boolean>(true);
  let loadError = $state<string | null>(null);
  let inspection = $state<LearningInspection | null>(null);
  let revision = $state<string>('');
  let token = $state<string>('');
  let config = $state<LearningConfig>({
    defaults: {},
    profile: null,
    profiles: {},
  });

  // Editor states
  let editorTarget = $state<string>('defaults');
  let selectedLanguage = $state<string>('');
  let newProfileName = $state<string>('');
  let drafts = $state<Record<string, Preferences>>({});

  // Conflict and error states
  let isSaving = $state<boolean>(false);
  let conflictError = $state<string | null>(null);
  let hasConflict = $state<boolean>(false);
  let validationError = $state<string | null>(null);
  let latestSavedPrefs = $state<Preferences | null>(null);
  let localStatus = $state<string | null>(null);

  let currentKey = $derived(`${editorTarget}::${selectedLanguage}`);

  let effectiveSelection = $derived<LearningSelection>(selection ?? defaultSelection());

  function getBasePrefsForTarget(cfg: LearningConfig, target: string, lang: string): Preferences {
    if (target === 'defaults') {
      return { ...cfg.defaults };
    }
    if (target.startsWith('profile:')) {
      const name = target.slice('profile:'.length);
      const prof = cfg.profiles[name];
      if (!prof) return {};
      if (lang) return { ...(prof.languages?.[lang] ?? {}) };
      return { ...prof.preferences };
    }
    return {};
  }

  let currentDraft = $derived<Preferences>(
    drafts[currentKey] !== undefined
      ? drafts[currentKey]
      : getBasePrefsForTarget(config, editorTarget, selectedLanguage)
  );

  let hasDraft = $derived<boolean>(drafts[currentKey] !== undefined);

  async function loadSettings(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const res = await inspectLearning(sessionId);
      inspection = res;
      revision = res.revision;
      token = res.token;
      config = res.config;
      if (!selection && res.selection) {
        onSelectionChange(res.selection);
      }
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : 'Failed to load personalization settings';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadSettings();
  });

  function handleActiveProfileChange(e: Event): void {
    const val = (e.target as HTMLSelectElement).value;
    onSelectionChange({
      ...effectiveSelection,
      profile: val === '' || val === '__none__' ? null : val,
      clear_profile: val === '__none__',
    });
  }

  function handleRepoDefaultsChange(e: Event): void {
    const checked = (e.target as HTMLInputElement).checked;
    onSelectionChange({
      ...effectiveSelection,
      use_repository_defaults: checked,
    });
  }

  function handleSessionPreferencesChange(updated: Preferences): void {
    onSelectionChange({
      ...effectiveSelection,
      session: updated,
    });
  }

  function handleResetSession(): void {
    onSelectionChange({
      ...effectiveSelection,
      session: {},
    });
    const msg = 'Cleared session preferences.';
    localStatus = msg;
    onStatusChange(msg);
  }

  function handleEditorTargetChange(e: Event): void {
    editorTarget = (e.target as HTMLSelectElement).value;
    if (editorTarget === 'defaults') selectedLanguage = '';
    validationError = null;
  }

  function handleLanguageChange(e: Event): void {
    selectedLanguage = (e.target as HTMLInputElement).value.trim();
    validationError = null;
  }

  function handleDraftChange(updated: Preferences): void {
    drafts[currentKey] = updated;
    conflictError = null;
    validationError = null;
  }

  function handleDiscardDraft(): void {
    delete drafts[currentKey];
    hasConflict = false;
    conflictError = null;
    latestSavedPrefs = null;
    const msg = 'Draft discarded.';
    localStatus = msg;
    onStatusChange(msg);
  }

  async function handleReviewLatest(): Promise<void> {
    try {
      const fresh = await inspectLearning(sessionId);
      inspection = fresh;
      revision = fresh.revision;
      token = fresh.token;
      config = fresh.config;
      latestSavedPrefs = getBasePrefsForTarget(fresh.config, editorTarget, selectedLanguage);
      const msg = 'Loaded latest settings from server. Review the differences below.';
      localStatus = msg;
      onStatusChange(msg);
    } catch (err: unknown) {
      conflictError = err instanceof Error ? err.message : 'Failed to reload latest settings';
    }
  }

  async function handleSave(): Promise<void> {
    isSaving = true;
    conflictError = null;
    validationError = null;

    try {
      const updatedConfig: LearningConfig = JSON.parse(JSON.stringify(config));

      if (editorTarget === 'defaults') {
        updatedConfig.defaults = { ...currentDraft };
      } else if (editorTarget.startsWith('profile:')) {
        const name = editorTarget.slice('profile:'.length);
        if (!updatedConfig.profiles[name]) {
          updatedConfig.profiles[name] = { preferences: {}, languages: {} };
        }
        if (selectedLanguage) {
          if (!updatedConfig.profiles[name].languages) {
            updatedConfig.profiles[name].languages = {};
          }
          updatedConfig.profiles[name].languages[selectedLanguage] = { ...currentDraft };
        } else {
          updatedConfig.profiles[name].preferences = { ...currentDraft };
        }
      } else if (editorTarget === 'new') {
        const name = newProfileName.trim();
        if (!name) {
          validationError = 'Profile name is required';
          isSaving = false;
          return;
        }
        if (Object.hasOwn(config.profiles, name)) {
          validationError = 'A learning profile with this name already exists. Choose another name.';
          return;
        }
        updatedConfig.profiles[name] = selectedLanguage
          ? { preferences: {}, languages: { [selectedLanguage]: { ...currentDraft } } }
          : { preferences: { ...currentDraft }, languages: {} };
      }

      const saved = await saveLearning(sessionId, revision, updatedConfig, token);
      revision = saved.revision;
      config = saved.config;
      onSaved?.();
      delete drafts[currentKey];
      hasConflict = false;
      latestSavedPrefs = null;

      if (editorTarget === 'new') {
        editorTarget = `profile:${newProfileName.trim()}`;
        newProfileName = '';
      }

      const msg = 'Saved learning preferences.';
      localStatus = msg;
      onStatusChange(msg);
    } catch (err: unknown) {
      if (err instanceof LearningApiError && err.status === 409) {
        hasConflict = true;
        conflictError = err.message || 'Settings changed elsewhere';
      } else {
        conflictError = err instanceof Error ? err.message : 'Failed to save preferences';
      }
    } finally {
      isSaving = false;
    }
  }

  async function handleSaveAs(): Promise<void> {
    const name = newProfileName.trim();
    if (!name) {
      validationError = 'Profile name is required to save as new profile';
      return;
    }
    if (Object.hasOwn(config.profiles, name)) {
      validationError = 'A learning profile with this name already exists. Choose another name.';
      return;
    }

    isSaving = true;
    conflictError = null;
    validationError = null;

    try {
      const updatedConfig: LearningConfig = JSON.parse(JSON.stringify(config));
      const source = editorTarget.startsWith('profile:') ? updatedConfig.profiles[editorTarget.slice(8)] : undefined;
      updatedConfig.profiles[name] = source ? JSON.parse(JSON.stringify(source)) : { preferences: {}, languages: {} };
      if (selectedLanguage) updatedConfig.profiles[name].languages[selectedLanguage] = { ...currentDraft };
      else updatedConfig.profiles[name].preferences = { ...currentDraft };

      const saved = await saveLearning(sessionId, revision, updatedConfig, token);
      revision = saved.revision;
      config = saved.config;
      onSaved?.();
      delete drafts[currentKey];
      hasConflict = false;
      latestSavedPrefs = null;
      editorTarget = `profile:${name}`;
      newProfileName = '';

      const msg = 'Saved learning preferences.';
      localStatus = msg;
      onStatusChange(msg);
    } catch (err: unknown) {
      if (err instanceof LearningApiError && err.status === 409) {
        hasConflict = true;
        conflictError = err.message || 'Settings changed elsewhere';
      } else {
        conflictError = err instanceof Error ? err.message : 'Failed to save preferences';
      }
    } finally {
      isSaving = false;
    }
  }

  async function handleMakeDefault(): Promise<void> {
    let targetProfile: string | null = null;
    if (editorTarget.startsWith('profile:')) {
      targetProfile = editorTarget.slice('profile:'.length);
    }

    isSaving = true;
    conflictError = null;
    validationError = null;

    try {
      const updatedConfig: LearningConfig = JSON.parse(JSON.stringify(config));
      updatedConfig.profile = targetProfile;

      const saved = await saveLearning(sessionId, revision, updatedConfig, token);
      revision = saved.revision;
      config = saved.config;
      hasConflict = false;
      onSaved?.();

      const msg = targetProfile
        ? `Made ${targetProfile} the default profile.`
        : 'Set user defaults as the default profile.';
      localStatus = msg;
      onStatusChange(msg);
    } catch (err: unknown) {
      if (err instanceof LearningApiError && err.status === 409) {
        hasConflict = true;
        conflictError = err.message || 'Settings changed elsewhere';
      } else {
        conflictError = err instanceof Error ? err.message : 'Failed to update default profile';
      }
    } finally {
      isSaving = false;
    }
  }
</script>

<section aria-labelledby="personalization-heading" class="page-container" id="learning-settings">
  <h1 id="personalization-heading" tabindex="-1">Personalization</h1>
  <p class="page-description">
    Customize how code explanations are presented. Configure focus, detail level, language assistance, and domain concepts.
  </p>

  {#if loading}
    <div class="workspace-status-card">
      <p>Loading personalization settings...</p>
    </div>
  {:else if loadError}
    <div class="workspace-error-card" role="alert">
      <p><strong>Error loading settings:</strong> {loadError}</p>
      <button type="button" class="btn-retry" onclick={loadSettings}>Retry</button>
    </div>
  {:else}
    <!-- Active profile & session choices -->
    <div class="settings-section">
      <h2>Active Learning Settings</h2>
      <p class="help-text">
        Learning profiles customize explanation focus and depth. Model profiles (configured in Settings or CLI) choose the model server and model.
      </p>

      <div class="form-group">
        <label for="learning-profile">Active learning profile</label>
        <select
          id="learning-profile"
          value={effectiveSelection.clear_profile ? '__none__' : (effectiveSelection.profile ?? '')}
          onchange={handleActiveProfileChange}
        >
          <option value="">{config.profile ? `Default (${config.profile})` : 'Default (User defaults)'}</option>
          <option value="__none__">None (Clear profile)</option>
          {#each Object.keys(config.profiles) as name}
            <option value={name}>{name}</option>
          {/each}
        </select>
        <p class="help-text">Selecting an active profile takes effect immediately for this session without saving.</p>
      </div>

      {#if inspection?.repository_available}
        <div class="form-group">
          <label class="checkbox-label" for="learning-use-repo-defaults">
            <input
              id="learning-use-repo-defaults"
              type="checkbox"
              checked={effectiveSelection.use_repository_defaults ?? false}
              onchange={handleRepoDefaultsChange}
            />
            <span>Use repository learning defaults if provided</span>
          </label>
        </div>
      {/if}

      <!-- Session Preferences Disclosure -->
      <details class="session-preferences-disclosure">
        <summary>Session preferences (temporary)</summary>
        <p class="help-text">
          Temporary adjustments for this session only. Prefix labels distinguish session settings from persisted profiles.
        </p>

        <PreferencesFields
          idPrefix="session-"
          labelPrefix="Session "
          value={effectiveSelection.session ?? {}}
          onChange={handleSessionPreferencesChange}
          allowInherit={true}
        />

        <div>
          <button type="button" class="btn-clear-session" onclick={handleResetSession}>
            Reset session
          </button>
        </div>
      </details>
    </div>

    <!-- Profile & Default Editor -->
    <div class="settings-section">
      <h2>Preferences Editor</h2>
      <p class="help-text">
        Edit persisted user defaults or custom named learning profiles. Changes require explicit saving.
      </p>

      {#if conflictError}
        <div class="workspace-error-card conflict-banner" role="alert">
          <p><strong>Settings Conflict:</strong> {conflictError}</p>
          {#if hasConflict}
            <p class="help-text">
              Settings changed elsewhere. Your typed draft has been preserved.
            </p>
            <div>
              <button type="button" class="btn-review" onclick={handleReviewLatest}>
                Review latest settings
              </button>
              <button type="button" class="btn-discard" onclick={handleDiscardDraft}>
                Discard draft
              </button>
            </div>
          {/if}
        </div>
      {/if}

      {#if validationError}
        <div class="workspace-error-card" role="alert">
          <p>{validationError}</p>
        </div>
      {/if}

      {#if latestSavedPrefs}
        <div class="workspace-status-card">
          <h3>Latest server preferences for this target:</h3>
          <ul>
            <li>Focus: {latestSavedPrefs.focus ?? '(inherit)'}</li>
            <li>Detail: {latestSavedPrefs.detail ?? '(inherit)'}</li>
            <li>Language help: {latestSavedPrefs.language_help ?? '(inherit)'}</li>
          </ul>
          <p class="help-text">
            Click "Save preferences" below to overwrite this target with your draft, or "Discard draft" to abandon your changes.
          </p>
        </div>
      {/if}

      {#if localStatus}
        <div id="learning-status" class="workspace-status-card">
          {localStatus}
        </div>
      {/if}

      <div class="form-group">
        <label for="editor-target">Editor target</label>
        <select id="editor-target" value={editorTarget} onchange={handleEditorTargetChange}>
          <option value="defaults">User defaults</option>
          {#each Object.keys(config.profiles) as name}
            <option value="profile:{name}">Profile: {name}</option>
          {/each}
          <option value="new">New profile</option>
        </select>
      </div>

      <div class="form-group">
        <label for="learning-name">{editorTarget === 'new' ? 'Profile name' : 'Profile name (for Save as new)'}</label>
        <input
          id="learning-name"
          type="text"
          placeholder="Profile name"
          value={newProfileName}
          oninput={(e) => {
            newProfileName = (e.target as HTMLInputElement).value;
            validationError = null;
          }}
        />
      </div>

      <div class="form-group">
        <label for="learning-language">Language override</label>
        <input
          id="learning-language"
          disabled={editorTarget === 'defaults'}
          list="supported-languages"
          placeholder="All languages (or e.g. rust, typescript)"
          value={selectedLanguage}
          oninput={(e) => {
            selectedLanguage = (e.target as HTMLInputElement).value.trim();
            validationError = null;
          }}
          onchange={handleLanguageChange}
        />
        <datalist id="supported-languages">
          <option value="rust">Rust</option>
          <option value="typescript">TypeScript</option>
          <option value="javascript">JavaScript</option>
          <option value="python">Python</option>
          <option value="go">Go</option>
          <option value="c">C</option>
          <option value="cpp">C++</option>
          <option value="csharp">C#</option>
          <option value="java">Java</option>
          <option value="hcl" label="Terraform / HCL">Terraform / HCL</option>
          <option value="yaml">YAML</option>
          <option value="json">JSON</option>
          <option value="jsonc">JSONC</option>
          <option value="dockerfile">Dockerfile</option>
          <option value="bash">Bash</option>
          <option value="powershell">PowerShell</option>
          <option value="kotlin">Kotlin</option>
        </datalist>
        <p class="help-text">Leave blank to edit general preferences for all languages.</p>
      </div>

      <div id="learning-editor">
        <PreferencesFields
          idPrefix=""
          labelPrefix=""
          value={currentDraft}
          onChange={handleDraftChange}
          allowInherit={true}
        />
      </div>

      <div class="editor-actions">
        <button
          type="button"
          id="learning-save"
          class="btn-save"
         
          disabled={isSaving}
          onclick={handleSave}
        >
          {isSaving ? 'Saving...' : 'Save preferences'}
        </button>

        <button
          type="button"
          id="learning-save-as"
          class="btn-save-as"
         
          disabled={isSaving}
          onclick={handleSaveAs}
        >
          Save as new profile
        </button>

        {#if editorTarget.startsWith('profile:')}
          <button
            type="button"
            id="learning-default"
            class="btn-default"
           
            disabled={isSaving}
            onclick={handleMakeDefault}
          >
            Make default
          </button>
        {/if}

        {#if hasDraft}
          <button
            type="button"
            class="btn-discard"
           
            disabled={isSaving}
            onclick={handleDiscardDraft}
          >
            Discard draft
          </button>
        {/if}
      </div>
    </div>
  {/if}
</section>
