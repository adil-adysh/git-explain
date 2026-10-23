import { test, expect } from '@playwright/test';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { startMock, createRepository, startApp, git } from './workspace-harness.mjs';
import AxeBuilder from '@axe-core/playwright';

async function workspace(page, mode = 'file', scenario = 'valid', cache = false) {
  const mock = await startMock(scenario);
  const repo = await createRepository();
  repo.source += '\nfn spare() -> bool { true }\n';
  await writeFile(path.join(repo.directory, 'src/lib.rs'), repo.source);
  await writeFile(path.join(repo.directory, 'src/other.rs'), 'fn another() {}\n');
  await writeFile(path.join(repo.directory, 'src/empty.rs'), '');
  git(repo.directory, 'add', '.');
  git(repo.directory, 'commit', '-qm', 'workspace fixture');
  const app = await startApp({ ...repo, modelUrl: mock.url, mode, cache });
  const requests = [];
  page.on('request', request => requests.push({ url: request.url(), method: request.method() }));
  await page.goto(app.url);
  return { ...app, mock, requests, repo };
}
async function selectCalculate(page) {
  await page.getByRole('link', { name: /src\/lib.rs/ }).click();
  await expect(page.getByRole('heading', { name: /lib.rs.*calculate/ })).toBeFocused();
}
async function modelCalls(mock) { return (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls; }

for (const mode of ['file', 'diff', 'daemon']) {
  test(`workspace ${mode}: lazy file/section navigation, source and history`, async ({ page }) => {
    const app = await workspace(page, mode);
    await expect(page.getByRole('navigation', { name: 'Application' })).toBeVisible();
    await expect(page.getByRole('main')).toHaveCount(1);
    await expect(page.getByRole('status')).toHaveCount(1);
    await expect(page.getByRole('link', { name: /src\/lib.rs/ })).toBeVisible();
    expect(app.requests.filter(r => r.url.includes('/ui/v1/sections/'))).toHaveLength(0);
    await page.getByRole('link', { name: /src\/lib.rs/ }).click();
    await expect(page.locator('[data-source-view]')).toBeVisible();
    expect(app.requests.filter(r => r.url.includes('/ui/v1/sections/'))).toHaveLength(1);
    await expect(page.getByRole('heading', { name: /lib.rs.*calculate/ })).toBeFocused();
    if (mode !== 'file') await page.getByRole('button', { name: 'All', exact: true }).click();
    await expect(page.getByRole('link', { name: /spare.*function/ })).toBeVisible();
    await expect(page.locator('[data-source-view]')).toHaveCount(1);
    expect(app.requests.filter(r => r.url.includes('/ui/v1/sections/'))).toHaveLength(1);
    await page.getByRole('link', { name: /spare.*function/ }).click();
    await expect(page.getByRole('heading', { name: /lib.rs.*spare/ })).toBeFocused();
    await page.goBack();
    await expect(page.getByRole('heading', { name: /lib.rs.*calculate/ })).toBeFocused();
    expect(app.requests.filter(r => r.url.includes('/ui/v1/sections/'))).toHaveLength(2);
    expect(await modelCalls(app.mock)).toBe(0);
  });
}

test('workspace filters retain reader and section navigation stops within file', async ({ page }) => {
  const app = await workspace(page);
  await selectCalculate(page);
  await expect(page.getByRole('button', { name: /Previous section/ })).toBeDisabled();
  await page.getByRole('textbox', { name: 'Filter sections' }).fill('spare');
  await expect(page.getByText('Not in current filter', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: /Next section/ })).toBeDisabled();
  await expect(page.getByRole('heading', { name: /lib.rs.*calculate/ })).toBeVisible();
  await page.getByRole('button', { name: 'Clear section filter' }).click();
  await page.getByRole('button', { name: /Next section: spare/ }).click();
  await expect(page.getByRole('button', { name: /Next section/ })).toBeDisabled();
  expect(await modelCalls(app.mock)).toBe(0);
});

for (const mode of ['file', 'diff', 'daemon']) {
  test(`workspace ${mode}: file totals distinguish changed sections from filtered sections`, async ({ page }) => {
    const app = await workspace(page, mode);
    const file = page.getByRole('link', { name: /src\/lib.rs/ });
    await expect(file).toContainText('2 sections');
    if (mode !== 'file') await expect(file).toContainText('1 changed · 1 unchanged');
    await selectCalculate(page);
    if (mode !== 'file') await page.getByRole('button', { name: 'All', exact: true }).click();
    await expect(page.getByText('Showing 2 of 2 code sections', { exact: true })).toBeVisible();
    await expect(page.getByRole('link', { name: /spare.*function/ })).toBeVisible();
    await page.getByRole('textbox', { name: 'Filter sections' }).fill('calculate');
    await expect(page.getByText('Showing 1 of 2 code sections', { exact: true })).toBeVisible();
    await expect(page.getByText('Section 1 of 1 shown (2 total)', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Clear section filter' }).click();
    await expect(page.getByText('Showing 2 of 2 code sections', { exact: true })).toBeVisible();
    if (mode !== 'file') {
      await page.getByRole('button', { name: 'Changed', exact: true }).click();
      await expect(page.getByText('Showing 1 of 2 code sections', { exact: true })).toBeVisible();
      await expect(page.getByRole('link', { name: /spare.*function/ })).toHaveCount(0);
      await expect(file).toContainText('2 sections');
      await expect(file).toContainText('1 changed · 1 unchanged');
      await page.getByRole('button', { name: 'All', exact: true }).click();
      await expect(page.getByRole('link', { name: /spare.*function/ })).toBeVisible();
    }
    expect(await modelCalls(app.mock)).toBe(0);
  });
}

for (const mode of ['file', 'diff', 'daemon']) {
  test(`workspace ${mode}: section filter defaults to changed only for diffs`, async ({ page }) => {
    const app = await workspace(page, mode);
    await selectCalculate(page);
    await expect(page.getByRole('button', { name: mode === 'file' ? 'All' : 'Changed', exact: true })).toHaveAttribute('aria-pressed', 'true');
    if (mode !== 'file') {
      await expect(page.getByRole('link', { name: /spare.*function/ })).toHaveCount(0);
      await expect(page.getByText('Showing 1 of 2 code sections', { exact: true })).toBeVisible();
      await page.getByRole('button', { name: 'All', exact: true }).click();
      await page.getByRole('link', { name: /spare.*function/ }).click();
      await page.reload();
      await expect(page.getByRole('button', { name: 'Changed', exact: true })).toHaveAttribute('aria-pressed', 'true');
      await expect(page.getByRole('heading', { name: /lib.rs.*spare/ })).toBeVisible();
      await expect(page.getByText('Not in current filter', { exact: true })).toBeVisible();
      await page.getByRole('link', { name: /src\/lib.rs/ }).click();
      await expect(page.getByRole('heading', { name: /lib.rs.*calculate/ })).toBeFocused();
    } else await expect(page.getByRole('link', { name: /spare.*function/ })).toBeVisible();
    expect(await modelCalls(app.mock)).toBe(0);
  });
}

test('workspace destinations preserve Explore selection and change appearance without requests', async ({ page }) => {
  const app = await workspace(page);
  await selectCalculate(page);
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeFocused();
  await page.getByLabel('Color Theme').selectOption('dark');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('link', { name: 'Explore', exact: true }).click();
  await expect(page.getByRole('heading', { name: /lib.rs.*calculate/ })).toBeVisible();
  expect(app.requests.filter(r => r.url.includes('/ui/v1/sections/'))).toHaveLength(1);
  expect(await modelCalls(app.mock)).toBe(0);
});

test('workspace explanation tabs use manual activation and never request on activation', async ({ page }) => {
  const app = await workspace(page);
  await selectCalculate(page);
  const overview = page.getByRole('tab', { name: 'Overview', exact: true });
  await expect(page.getByRole('button', { name: 'Generate Overview', exact: true })).toBeEnabled();
  await overview.focus();
  const before = app.requests.length;
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('tab', { name: 'Concepts', exact: true })).toBeFocused();
  await expect(overview).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('End');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('tab', { name: 'Deep', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('tab', { name: 'Changes', exact: true })).toHaveCount(0);
  expect(app.requests).toHaveLength(before);
  expect(await modelCalls(app.mock)).toBe(0);
});

test('workspace late source response cannot replace the current section', async ({ page }) => {
  const app = await workspace(page);
  let first = true;
  await page.route('**/ui/v1/sections/**', async route => {
    if (first) {
      first = false;
      const response = await route.fetch();
      await new Promise(resolve => setTimeout(resolve, 250));
      await route.fulfill({ response }).catch(() => {});
    } else await route.continue();
  });
  await page.getByRole('link', { name: /src\/lib.rs/ }).click();
  await page.getByRole('link', { name: /spare.*function/ }).click();
  await expect(page.getByRole('heading', { name: /lib.rs.*spare/ })).toBeFocused();
  await expect(page.locator('[data-source-view]')).toContainText('fn spare()');
  expect(await modelCalls(app.mock)).toBe(0);
});

test('workspace supports narrow forced-color reduced-motion reading', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 800 });
  await page.emulateMedia({ forcedColors: 'active', reducedMotion: 'reduce' });
  const app = await workspace(page);
  await selectCalculate(page);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await expect(page.getByRole('link', { name: 'Settings', exact: true })).toBeVisible();
  expect(await modelCalls(app.mock)).toBe(0);
});

test('workspace failed source can be retried without inference', async ({ page }) => {
  const app = await workspace(page);
  let fail = true;
  await page.route('**/ui/v1/sections/**', async route => {
    if (fail) {
      fail = false;
      await route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ ok: false, code: 'unavailable', error: 'Source temporarily unavailable', retryable: true }) });
    } else await route.continue();
  });
  await page.getByRole('link', { name: /src\/lib.rs/ }).click();
  await expect(page.getByRole('heading', { name: 'Source could not be loaded' })).toBeFocused();
  await page.getByRole('button', { name: 'Retry loading source' }).click();
  await expect(page.locator('[data-source-view]')).toContainText('fn calculate()');
  expect(await modelCalls(app.mock)).toBe(0);
});

test('workspace generates only the explicit view and retains independent results', async ({ page }) => {
  const app = await workspace(page);
  await selectCalculate(page);
  await page.getByRole('button', { name: 'Generate Overview', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Regenerate Overview', exact: true })).toBeEnabled();
  expect(await modelCalls(app.mock)).toBe(1);
  await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
  expect(await modelCalls(app.mock)).toBe(1);
  await page.getByRole('button', { name: 'Generate Concepts', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Regenerate Concepts', exact: true })).toBeEnabled();
  await expect(page.locator('[data-source-view]')).toHaveCount(1);
  await expect(page.getByRole('list', { name: 'Source annotations' })).toBeVisible();
  await page.getByRole('tab', { name: 'Overview', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Regenerate Overview', exact: true })).toBeEnabled();
  expect(await modelCalls(app.mock)).toBe(2);
});

test('workspace profile conflict preserves the typed draft', async ({ page }) => {
  const app = await workspace(page);
  await page.getByRole('link', { name: 'Personalization', exact: true }).click();
  await page.getByLabel('Editor target').selectOption('defaults');
  await page.getByLabel('Focus', { exact: true }).selectOption('architecture');
  await page.route('**/api/learning', async route => {
    if (route.request().method() === 'PUT') await route.fulfill({ status: 409, contentType: 'application/json', body: JSON.stringify({ ok: false, error: 'Settings changed elsewhere' }) });
    else await route.continue();
  });
  await page.getByRole('button', { name: 'Save preferences', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Settings changed elsewhere');
  await expect(page.getByLabel('Focus', { exact: true })).toHaveValue('architecture');
  expect(await modelCalls(app.mock)).toBe(0);
});

test('workspace new profile cannot overwrite an existing profile', async ({ page }) => {
  const app = await workspace(page);
  await page.getByRole('link', { name: 'Personalization', exact: true }).click();
  await page.getByLabel('Editor target').selectOption('new');
  await page.getByRole('textbox', { name: 'Profile name', exact: true }).fill('learner');
  await page.getByLabel('Focus', { exact: true }).selectOption('syntax');
  await page.getByRole('button', { name: 'Save preferences', exact: true }).click();
  await expect(page.getByLabel('Editor target')).toHaveValue('profile:learner');
  await page.getByLabel('Editor target').selectOption('new');
  await page.getByRole('textbox', { name: 'Profile name', exact: true }).fill('learner');
  await page.getByLabel('Focus', { exact: true }).selectOption('architecture');
  await page.getByRole('button', { name: 'Save preferences', exact: true }).click();
  await expect(page.getByText('A learning profile with this name already exists. Choose another name.', { exact: true })).toBeVisible();
  await page.getByLabel('Editor target').selectOption('profile:learner');
  await page.getByLabel('Language override').fill('rust');
  await page.getByLabel('Language override').blur();
  await expect(page.getByLabel('Focus', { exact: true })).toHaveValue('');
  expect(await modelCalls(app.mock)).toBe(0);
});

test('workspace concept feedback stays temporary until an explicit profile save', async ({ page }) => {
  const app = await workspace(page);
  await page.getByRole('link', { name: 'Personalization', exact: true }).click();
  await page.getByLabel('Editor target').selectOption('new');
  await page.getByRole('textbox', { name: 'Profile name', exact: true }).fill('learner');
  await page.getByRole('button', { name: 'Save preferences', exact: true }).click();
  await expect(page.getByLabel('Editor target')).toHaveValue('profile:learner');
  await page.getByLabel('Active learning profile').selectOption('learner');
  await page.getByRole('link', { name: 'Explore', exact: true }).click();
  await selectCalculate(page);
  await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await page.getByRole('button', { name: 'Generate Concepts', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Regenerate Concepts', exact: true })).toBeEnabled();
  const annotations = page.getByRole('list', { name: 'Source annotations' });
  const term = await annotations.getByRole('heading').first().textContent();
  await annotations.getByRole('button', { name: 'I know this already', exact: true }).first().click();
  expect(app.requests.filter(request => request.method === 'PUT')).toHaveLength(1);
  await page.getByRole('button', { name: 'Choose profile for feedback', exact: true }).click();
  await page.getByLabel('Save feedback to learning profile').selectOption('learner');
  await page.getByRole('button', { name: 'Save feedback to profile', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('Feedback saved');
  const saved = await fetch(`${app.url}/api/learning`).then(response => response.json());
  expect(saved.config.profiles.learner.preferences.known_concepts).toContain(term);
  expect(saved.config.profiles.learner.preferences.learning_concepts ?? []).not.toContain(term);
  expect(await modelCalls(app.mock)).toBe(1);
});

test('workspace feedback save completes for its original section after navigation', async ({ page }) => {
  const app = await workspace(page);
  await page.getByRole('link', { name: 'Personalization', exact: true }).click();
  await page.getByLabel('Editor target').selectOption('new');
  await page.getByRole('textbox', { name: 'Profile name', exact: true }).fill('learner');
  await page.getByRole('button', { name: 'Save preferences', exact: true }).click();
  await expect(page.getByLabel('Editor target')).toHaveValue('profile:learner');
  await page.getByLabel('Active learning profile').selectOption('learner');
  await page.getByRole('link', { name: 'Explore', exact: true }).click();
  await selectCalculate(page);
  await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await page.getByRole('button', { name: 'Generate Concepts', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Regenerate Concepts', exact: true })).toBeEnabled();
  const annotations = page.getByRole('list', { name: 'Source annotations' });
  const term = await annotations.getByRole('heading').first().textContent();
  await annotations.getByRole('button', { name: 'I know this already', exact: true }).first().click();
  expect(app.requests.filter(request => request.method === 'PUT')).toHaveLength(1);
  await page.getByRole('button', { name: 'Choose profile for feedback', exact: true }).click();
  await page.getByLabel('Save feedback to learning profile').selectOption('learner');
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  await page.route('**/api/learning', async route => {
    if (route.request().method() === 'PUT') { await gate; }
    await route.continue();
  });
  await page.getByRole('button', { name: 'Save feedback to profile', exact: true }).click();
  await page.getByRole('link', { name: /spare.*function/ }).click();
  release();
  await expect.poll(async () => (await fetch(`${app.url}/api/learning`).then(r => r.json())).config.profiles.learner.preferences.known_concepts ?? []).toContain(term);
  await page.getByRole('link', { name: /calculate.*function/ }).click();
  await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Choose profile for feedback', exact: true })).toHaveCount(0);
  const saved = await fetch(`${app.url}/api/learning`).then(response => response.json());
  expect(saved.config.profiles.learner.preferences.known_concepts).toContain(term);
  expect(saved.config.profiles.learner.preferences.learning_concepts ?? []).not.toContain(term);
  expect(await modelCalls(app.mock)).toBe(1);
});

for (const mode of ['file', 'diff', 'daemon']) {
  test(`workspace ${mode}: Deep uses one request and later context invalidates only dependencies`, async ({ page }) => {
    const app = await workspace(page, mode);
    await selectCalculate(page);
    await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
    await page.getByRole('button', { name: 'Generate Concepts', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Regenerate Concepts', exact: true })).toBeEnabled();
    await page.getByRole('tab', { name: 'Deep', exact: true }).click();
    expect(await modelCalls(app.mock)).toBe(1);
    await page.getByRole('button', { name: 'Generate Deep', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Regenerate Deep', exact: true })).toBeEnabled();
    expect(await modelCalls(app.mock)).toBe(2);
    await page.getByRole('tab', { name: mode === 'file' ? 'Overview' : 'Changes', exact: true }).click();
    await page.getByRole('button', { name: `Generate ${mode === 'file' ? 'Overview' : 'Changes'}`, exact: true }).click();
    await expect.poll(() => modelCalls(app.mock)).toBe(3);
    await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Regenerate Concepts', exact: true })).toBeEnabled();
    await page.getByRole('tab', { name: 'Deep', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Generate Deep', exact: true })).toBeEnabled();
  });
}

test('workspace explicit jobs survive navigation and model work stays serialized', async ({ page }) => {
  const app = await workspace(page, 'file', 'controlled_valid');
  await selectCalculate(page);
  await expect(page.getByRole('button', { name: 'Generate Overview', exact: true })).toBeEnabled();
  await fetch(`${app.mock.origin}/hold`, { method: 'POST' });
  await page.getByRole('button', { name: 'Generate Overview', exact: true }).click();
  await expect.poll(() => modelCalls(app.mock)).toBe(1);
  await page.getByRole('tab', { name: 'Deep', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Generate Deep', exact: true })).toBeDisabled();
  await page.getByRole('link', { name: /spare.*function/ }).click();
  await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await page.getByRole('button', { name: 'Generate Concepts', exact: true }).click();
  expect(await modelCalls(app.mock)).toBe(1);
  await fetch(`${app.mock.origin}/release`, { method: 'POST' });
  await expect(page.getByRole('button', { name: 'Regenerate Concepts', exact: true })).toBeEnabled();
  await expect(page.getByRole('heading', { name: /lib.rs.*spare/ })).toBeVisible();
  const calls = await fetch(`${app.mock.origin}/calls`).then(response => response.json());
  expect(calls.calls).toBe(2); expect(calls.maxActive).toBe(1);
  await page.getByRole('link', { name: /calculate.*function/ }).click();
  await expect(page.getByRole('button', { name: 'Regenerate Overview', exact: true })).toBeEnabled();
  expect(await modelCalls(app.mock)).toBe(2);
});

test('workspace section and view overrides remain independent and preserve used settings', async ({ page }) => {
  const app = await workspace(page);
  await selectCalculate(page);
  await page.getByText('Customize this section', { exact: true }).click();
  await page.getByLabel('Language help', { exact: true }).selectOption('explain');
  await page.getByLabel('Focus', { exact: true }).selectOption('execution');
  await page.getByRole('button', { name: 'Apply temporary override' }).click();
  await page.getByLabel('Apply to').selectOption('view');
  await page.getByLabel('Focus', { exact: true }).selectOption('reasoning');
  await page.getByLabel('Detail', { exact: true }).selectOption('detailed');
  await page.getByRole('button', { name: 'Apply temporary override' }).click();
  await page.getByRole('button', { name: 'Generate Overview', exact: true }).click();
  await page.getByText('Explanation preferences', { exact: true }).click();
  await expect(page.getByText('Used for this explanation: Explain language features · Reasoning focus · Detailed explanation', { exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await page.getByRole('button', { name: 'Check saved explanation', exact: true }).click();
  await expect(page.getByText('For the next explanation: Explain language features · Execution focus · Standard detail', { exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Overview', exact: true }).click();
  await page.getByRole('button', { name: 'Reset this scope', exact: true }).click();
  await expect(page.getByText('For the next explanation: Explain language features · Execution focus · Standard detail', { exact: true })).toBeVisible();
  await expect(page.getByText('Used for this explanation: Explain language features · Reasoning focus · Detailed explanation', { exact: true })).toBeVisible();
  expect(await modelCalls(app.mock)).toBe(1);
});

test('workspace explanation preferences are optional details without a misleading pending notice', async ({ page }) => {
  const app = await workspace(page);
  await selectCalculate(page);
  await expect(page.getByRole('button', { name: 'Generate Overview', exact: true })).toBeEnabled();
  await expect(page.getByRole('tabpanel')).not.toContainText('Pending settings');
  const next = page.getByText(/^For the next explanation:/);
  await expect(next).not.toBeVisible();
  await page.getByText('Explanation preferences', { exact: true }).click();
  await expect(next).toBeVisible();
  expect(await modelCalls(app.mock)).toBe(0);
  await page.getByRole('button', { name: 'Generate Overview', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Regenerate Overview', exact: true })).toBeEnabled();
  await expect(next).toHaveCount(0);
  await expect(page.getByText(/^Used for this explanation:/)).toBeVisible();
  await page.getByText('Explanation preferences', { exact: true }).click();
  await expect(page.getByText(/^Used for this explanation:/)).not.toBeVisible();
  expect(await modelCalls(app.mock)).toBe(1);
});

test('workspace daemon notices changed and expired comparisons without generating', async ({ page }) => {
  const app = await workspace(page, 'daemon');
  await selectCalculate(page);
  await writeFile(path.join(app.repo.directory, 'src/lib.rs'), app.repo.source.replace('let first = 1;', 'let first = 20;'));
  const refresh = await fetch(`${app.origin}/api/repos/refresh`, { method: 'POST', headers: { 'x-git-explain-control-token': app.controlToken } });
  expect((await refresh.json()).ok).toBe(true);
  await expect(page.getByRole('button', { name: 'Reload files', exact: true })).toBeVisible({ timeout: 8000 });
  await expect(page.getByRole('button', { name: 'Generate Overview', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Reload files', exact: true }).click();
  await selectCalculate(page);
  await expect(page.locator('[data-source-view]')).toContainText('let first = 20;');
  await page.route('**/ui/v1/workspace', route => route.fulfill({ status: 410, contentType: 'application/json', body: JSON.stringify({ ok: false, code: 'session_expired' }) }));
  await expect(page.getByText('This repository session expired. Reopen it with git explain diff.', { exact: true }).first()).toBeVisible({ timeout: 8000 });
  expect(await modelCalls(app.mock)).toBe(0);
});

test('workspace hiding explanations also hides later source annotations', async ({ page }) => {
  const app = await workspace(page);
  await selectCalculate(page);
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByLabel('Hide explanations and annotations').check();
  await page.getByRole('link', { name: 'Explore', exact: true }).click();
  await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await page.getByRole('button', { name: 'Generate Concepts', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Regenerate Concepts', exact: true })).toBeEnabled();
  await expect(page.getByRole('list', { name: 'Source annotations' })).toHaveCount(0);
  await expect(page.locator('.annotated-line')).toHaveCount(0);
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByLabel('Hide explanations and annotations').uncheck();
  await page.getByRole('link', { name: 'Explore', exact: true }).click();
  await expect(page.getByRole('list', { name: 'Source annotations' })).toBeVisible();
  expect(await modelCalls(app.mock)).toBe(1);
});

test('workspace automated accessibility covers Explore, Personalization and Settings', async ({ page }) => {
  await workspace(page);
  await selectCalculate(page);
  await expect(page.getByRole('button', { name: 'Generate Overview', exact: true })).toBeEnabled();
  for (const destination of ['Explore', 'Personalization', 'Settings']) {
    await page.getByRole('link', { name: destination, exact: true }).click();
    if (destination === 'Personalization') await expect(page.getByLabel('Editor target')).toBeVisible();
    const result = await new AxeBuilder({ page }).analyze();
    expect(result.violations).toEqual([]);
  }
});

test('workspace model failure preserves readable source and allows explicit retry', async ({ page }) => {
  const app = await workspace(page);
  await selectCalculate(page);
  let fail = true;
  await page.route('**/api/file-units/*/explain', async route => {
    if (fail) { fail = false; await route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ ok: false, error: 'The model is temporarily unavailable. Try again.' }) }); }
    else await route.continue();
  });
  const generate = page.getByRole('button', { name: 'Generate Overview', exact: true });
  await generate.click();
  await expect(page.getByRole('status')).toContainText('Overview explanation failed');
  await expect(generate).toBeFocused();
  await expect(page.locator('[data-source-view]')).toContainText('fn calculate()');
  expect(await modelCalls(app.mock)).toBe(0);
  await generate.click();
  await expect(page.getByRole('button', { name: 'Regenerate Overview', exact: true })).toBeEnabled();
  expect(await modelCalls(app.mock)).toBe(1);
});

 test('workspace file selection opens first section and restores last read section', async ({ page }) => {
  const app = await workspace(page);
  await page.getByRole('link', { name: /src\/lib.rs/ }).click();
  await expect(page.getByRole('heading', { name: /lib.rs.*calculate/ })).toBeFocused();
  await page.getByRole('link', { name: /spare.*function/ }).click();
  await expect(page.getByRole('heading', { name: /lib.rs.*spare/ })).toBeFocused();
  await page.getByRole('link', { name: /src\/other.rs/ }).click();
  await expect(page.getByRole('heading', { name: /other.rs.*another/ })).toBeFocused();
  await page.getByRole('link', { name: /src\/lib.rs/ }).click();
  await expect(page.getByRole('heading', { name: /lib.rs.*spare/ })).toBeFocused();
  expect(new URL(page.url()).searchParams.get('section')).toBeTruthy();
  expect(app.requests.filter(r => r.url.includes('/ui/v1/sections/'))).toHaveLength(3);
  await page.goBack();
  await expect(page.getByRole('heading', { name: /other.rs.*another/ })).toBeFocused();
  expect(await modelCalls(app.mock)).toBe(0);
});

test('workspace plain source retains annotations and exposes Git diff and previous source', async ({ page }) => {
  const app = await workspace(page, 'diff');
  await selectCalculate(page);
  await page.getByRole('tab', { name: 'Changes', exact: true }).click();
  await page.getByRole('button', { name: 'Generate Changes', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Regenerate Changes', exact: true })).toBeEnabled();
  await page.getByRole('link', { name: 'Settings', exact: true }).click();
  await page.getByLabel('Read source as plain text').check();
  await page.getByRole('link', { name: 'Explore', exact: true }).click();
  const source = page.getByRole('textbox', { name: 'Source code', exact: true });
  await expect(source).toHaveValue(/let first = 10;/);
  await page.getByRole('list', { name: 'Source annotations' }).getByRole('link').first().click();
  await expect(source).toBeFocused();
  await page.getByText('View diff (', { exact: false }).click();
  await expect(page.getByRole('textbox', { name: 'Diff', exact: true })).toHaveValue(/let first = 10;/);
  await page.getByText('View previous source (before comparison)', { exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Previous source', exact: true })).toHaveValue(/let first = 1;/);
  await expect(page.locator('[data-source-view]')).toHaveCount(1);
  expect(await modelCalls(app.mock)).toBe(1);
});

test('workspace selecting an empty file explains why no source is loaded', async ({ page }) => {
  const app = await workspace(page);
  await page.getByRole('link', { name: /src\/empty.rs/ }).click();
  await expect(page.getByRole('heading', { name: 'No code sections', exact: true })).toBeVisible();
  await expect(page.locator('[data-source-view]')).toHaveCount(0);
  expect(app.requests.filter(r => r.url.includes('/ui/v1/sections/'))).toHaveLength(0);
  expect(await modelCalls(app.mock)).toBe(0);
});

for (const mode of ['file', 'diff', 'daemon']) {
  test(`workspace ${mode}: completion and errors stay in their requested view`, async ({ page }) => {
    const app = await workspace(page, mode, 'controlled_valid');
    await selectCalculate(page);
    await fetch(`${app.mock.origin}/hold`, { method: 'POST' });
    await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
    await page.getByRole('button', { name: 'Generate Concepts', exact: true }).click();
    await expect.poll(() => modelCalls(app.mock)).toBe(1);
    await page.getByRole('tab', { name: 'Overview', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Generate Overview', exact: true })).toBeDisabled();
    await fetch(`${app.mock.origin}/release`, { method: 'POST' });
    await expect(page.getByRole('status')).toContainText('Concepts explanation ready');
    await expect(page.getByRole('tab', { name: 'Overview', exact: true })).toHaveAttribute('aria-selected', 'true');
    await expect(page.locator('.annotated-line')).toHaveCount(0);
    await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
    await expect(page.getByRole('list', { name: 'Source annotations' })).toBeVisible();
    await page.route('**/units/*/concepts/regenerate', route => route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ ok: false, error: 'Concept request unavailable' }) }));
    await page.route('**/file-units/*/concepts/regenerate', route => route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ ok: false, error: 'Concept request unavailable' }) }));
    await page.getByRole('button', { name: 'Regenerate Concepts', exact: true }).click();
    await expect(page.getByRole('tabpanel')).toContainText('Concept request unavailable');
    await expect(page.getByRole('list', { name: 'Source annotations' })).toBeVisible();
    await page.getByRole('tab', { name: 'Deep', exact: true }).click();
    await expect(page.getByRole('tabpanel')).not.toContainText('Concept request unavailable');
    await expect(page.locator('.annotated-line')).toHaveCount(0);
    expect(await modelCalls(app.mock)).toBe(1);
  });

  test(`workspace ${mode}: all views restore after reload without tab requests`, async ({ page }) => {
    const app = await workspace(page, mode, 'valid', true);
    await selectCalculate(page);
    const kinds = mode === 'file' ? ['Overview', 'Concepts', 'Deep'] : ['Overview', 'Changes', 'Concepts', 'Deep'];
    for (const kind of kinds) {
      await page.getByRole('tab', { name: kind, exact: true }).click();
      await page.getByRole('button', { name: `Generate ${kind}`, exact: true }).click();
      await expect(page.getByRole('button', { name: `Regenerate ${kind}`, exact: true })).toBeEnabled();
    }
    const calls = await modelCalls(app.mock);
    expect(calls).toBe(kinds.length);
    await page.reload();
    await expect(page.getByRole('button', { name: 'Regenerate Deep', exact: true })).toBeEnabled();
    // Wait for the selected section's saved results, then ensure navigation is local.
    await expect.poll(async () => page.getByRole('button', { name: 'Check saved explanation', exact: true }).isEnabled()).toBe(true);
    const before = app.requests.length;
    for (const kind of kinds) {
      await page.getByRole('tab', { name: kind, exact: true }).click();
      await expect(page.getByRole('button', { name: `Regenerate ${kind}`, exact: true })).toBeEnabled();
      await expect(page.locator('[data-source-view]')).toHaveCount(1);
    }
    expect(app.requests).toHaveLength(before);
    expect(await modelCalls(app.mock)).toBe(calls);
  });

  test(`workspace ${mode}: annotations follow the active view on one source`, async ({ page }) => {
    const app = await workspace(page, mode);
    await selectCalculate(page);
    for (const kind of mode === 'file' ? ['Concepts'] : ['Changes', 'Concepts']) {
      await page.getByRole('tab', { name: kind, exact: true }).click();
      await page.getByRole('button', { name: `Generate ${kind}`, exact: true }).click();
      await expect(page.getByRole('button', { name: `Regenerate ${kind}`, exact: true })).toBeEnabled();
      const annotations = page.getByRole('list', { name: 'Source annotations' });
      await expect(annotations).toBeVisible();
      await annotations.getByRole('link').first().click();
      await expect(page.locator('[data-source-view] :focus')).toHaveClass('annotated-line');
      await expect(page.locator('[data-source-view]')).toHaveCount(1);
      await page.getByRole('tab', { name: 'Overview', exact: true }).click();
      await expect(page.locator('.annotated-line')).toHaveCount(0);
      await expect(annotations).toHaveCount(0);
      await page.getByRole('tab', { name: kind, exact: true }).click();
      await expect(annotations).toBeVisible();
    }
    if (mode !== 'file') await page.getByRole('button', { name: 'All', exact: true }).click();
    await page.getByRole('link', { name: /spare.*function/ }).click();
    await expect(page.locator('.annotated-line')).toHaveCount(0);
    if (mode !== 'file') {
      await expect(page.getByText('This unchanged code section is available for reading.', { exact: true })).toBeVisible();
      await expect(page.getByRole('tab')).toHaveCount(0);
    }
    expect(await modelCalls(app.mock)).toBe(mode === 'file' ? 1 : 2);
  });
}

for (const mode of ['file', 'diff', 'daemon']) {
  test(`workspace ${mode}: invalid source evidence uses one request and explicit retry`, async ({ page }) => {
    const app = await workspace(page, mode, 'invalid_evidence_then_valid');
    await selectCalculate(page);
    await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
    await page.getByRole('button', { name: 'Generate Concepts', exact: true }).click();
    await expect(page.getByRole('tabpanel')).toContainText('The explanation did not match the selected source');
    expect(await modelCalls(app.mock)).toBe(1);
    await expect(page.getByRole('list', { name: 'Source annotations' })).toHaveCount(0);
    await expect(page.locator('[data-source-view]')).toHaveCount(1);
    await page.getByRole('tab', { name: 'Overview', exact: true }).click();
    await page.getByRole('tab', { name: 'Concepts', exact: true }).click();
    expect(await modelCalls(app.mock)).toBe(1);
    await page.getByRole('button', { name: 'Generate Concepts', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Regenerate Concepts', exact: true })).toBeEnabled();
    await expect(page.getByRole('list', { name: 'Source annotations' })).toBeVisible();
    expect(await modelCalls(app.mock)).toBe(2);
  });
}
