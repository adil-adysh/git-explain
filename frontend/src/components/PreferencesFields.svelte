<script lang="ts">
  import type { Preferences } from '../learning';

  type ListKey =
    | 'known_languages'
    | 'learning_languages'
    | 'known_frameworks'
    | 'learning_frameworks'
    | 'known_domains'
    | 'learning_domains'
    | 'known_concepts'
    | 'learning_concepts';

  interface Props {
    idPrefix?: string;
    labelPrefix?: string;
    value: Preferences;
    onChange: (value: Preferences) => void;
    allowInherit?: boolean;
  }

  let {
    idPrefix = '',
    labelPrefix = '',
    value,
    onChange,
    allowInherit = true,
  }: Props = $props();

  const listFields: Array<{ key: ListKey; label: string }> = [
    { key: 'known_languages', label: 'Known languages' },
    { key: 'learning_languages', label: 'Learning languages' },
    { key: 'known_frameworks', label: 'Known frameworks' },
    { key: 'learning_frameworks', label: 'Learning frameworks' },
    { key: 'known_domains', label: 'Known domains' },
    { key: 'learning_domains', label: 'Learning domains' },
    { key: 'known_concepts', label: 'Known concepts' },
    { key: 'learning_concepts', label: 'Learning concepts' },
  ];

  let listTexts = $state<Record<string, string>>({});

  function parseList(text: string): string[] {
    return text
      .split(/[\n,]+/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
  }

  function arraysEqual(a: string[] | null | undefined, b: string[] | null | undefined): boolean {
    if (a == null && b == null) return true;
    if (a == null || b == null) return false;
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (a[i] !== b[i]) return false;
    }
    return true;
  }

  $effect(() => {
    for (const { key } of listFields) {
      const currentVal = value[key];
      const currentText = listTexts[key] ?? '';
      if (currentVal == null) {
        if (currentText !== '') {
          listTexts[key] = '';
        }
      } else {
        const parsed = parseList(currentText);
        if (!arraysEqual(parsed, currentVal)) {
          listTexts[key] = currentVal.join(', ');
        }
      }
    }
  });

  function onScalarChange(field: 'focus' | 'detail' | 'language_help', e: Event): void {
    const val = (e.target as HTMLSelectElement).value;
    const updated: Preferences = {
      ...value,
      [field]: val === '' ? null : (val as Preferences[typeof field]),
    };
    onChange(updated);
  }

  function isInherited(key: ListKey): boolean {
    return value[key] == null;
  }

  function toggleInherit(key: ListKey, inherit: boolean): void {
    if (inherit) {
      listTexts[key] = '';
      onChange({
        ...value,
        [key]: null,
      });
    } else {
      listTexts[key] = '';
      onChange({
        ...value,
        [key]: [],
      });
    }
  }

  function handleListInput(key: ListKey, text: string): void {
    listTexts[key] = text;
    const parsed = parseList(text);
    onChange({
      ...value,
      [key]: parsed,
    });
  }
</script>

<div class="preferences-fields">
  <div class="form-group">
    <label for="{idPrefix}focus">{labelPrefix}Focus</label>
    <select
      id="{idPrefix}focus"
      data-preference="focus"
      value={value.focus ?? ''}
      onchange={(e) => onScalarChange('focus', e)}
    >
      {#if allowInherit}
        <option value="">Inherit</option>
      {/if}
      <option value="balanced">balanced</option>
      <option value="syntax">syntax</option>
      <option value="execution">execution</option>
      <option value="reasoning">reasoning</option>
      <option value="architecture">architecture</option>
    </select>
  </div>

  <div class="form-group">
    <label for="{idPrefix}detail">{labelPrefix}Detail</label>
    <select
      id="{idPrefix}detail"
      data-preference="detail"
      value={value.detail ?? ''}
      onchange={(e) => onScalarChange('detail', e)}
    >
      {#if allowInherit}
        <option value="">Inherit</option>
      {/if}
      <option value="concise">concise</option>
      <option value="balanced">balanced</option>
      <option value="detailed">detailed</option>
    </select>
  </div>

  <div class="form-group">
    <label for="{idPrefix}language_help">{labelPrefix}Language help</label>
    <select
      id="{idPrefix}language_help"
      data-preference="language_help"
      value={value.language_help ?? ''}
      onchange={(e) => onScalarChange('language_help', e)}
    >
      {#if allowInherit}
        <option value="">Inherit</option>
      {/if}
      <option value="automatic">automatic</option>
      <option value="explain">explain</option>
      <option value="brief">brief</option>
      <option value="skip">skip</option>
    </select>
  </div>

  {#each listFields as { key, label }}
    <div class="form-group list-group">
      <div class="list-header">
        <label for="{idPrefix}{key}">{labelPrefix}{label}</label>
        {#if allowInherit}
          <label class="checkbox-label" for="{idPrefix}{key}-inherit">
            <input
              id="{idPrefix}{key}-inherit"
              type="checkbox"
              data-list-inherit={key}
              checked={isInherited(key)}
              onchange={(e) => toggleInherit(key, (e.target as HTMLInputElement).checked)}
            />
            <span>{labelPrefix}Inherit {label.toLowerCase()}</span>
          </label>
        {/if}
      </div>
      <textarea
        id="{idPrefix}{key}"
        data-list={key}
        rows="2"
        placeholder={allowInherit && isInherited(key) ? 'Inherited (type to override)' : 'Comma-separated or newline-separated'}
        value={listTexts[key] ?? ''}
        oninput={(e) => handleListInput(key, (e.target as HTMLTextAreaElement).value)}
      ></textarea>
    </div>
  {/each}
</div>
