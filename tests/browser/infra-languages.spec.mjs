import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { startMock, createRepository, startApp, git } from './workspace-harness.mjs';

// Original synthetic configuration/scripts; none of this input is executed.
const fixtures = JSON.parse(await readFile(new URL('../fixtures/infra-languages.json', import.meta.url), 'utf8'));
const calls = async mock => (await fetch(`${mock.origin}/calls`).then(response => response.json())).calls;

for (const mode of ['file', 'diff', 'daemon']) {
  for (const fixture of fixtures) {
    test(`infrastructure ${mode} ${fixture.name}: counts, views, source and explicit generation`, async ({ page }) => {
      const mock = await startMock('valid');
      const repo = await createRepository();
      const filePath = path.join(repo.directory, fixture.path);
      await mkdir(path.dirname(filePath), { recursive: true });
      await writeFile(filePath, fixture.before);
      git(repo.directory, 'add', '.');
      git(repo.directory, 'commit', '-qm', 'synthetic infrastructure baseline');
      await writeFile(filePath, fixture.after);
      const app = await startApp({ ...repo, modelUrl: mock.url, mode, cache: true });
      const apiBase = app.session
        ? `${app.origin}/api/sessions/${app.session}/ui/v1`
        : `${app.url}/api/ui/v1`;
      const requests = [];
      page.on('request', request => requests.push({ url: request.url(), method: request.method() }));
      await page.goto(app.url);

      const indexResponse = await page.request.get(`${apiBase}/index`);
      expect(indexResponse.ok()).toBe(true);
      const index = await indexResponse.json();
      const file = index.files.find(value => value.path_label === fixture.path);
      expect(file, `${fixture.path} must be discoverable in ${mode}`).toBeTruthy();
      expect(file.language).toBe(fixture.language);
      expect(file.section_count).toBe(file.sections.length);
      if (mode !== 'file') {
        expect(file.sections.filter(section => section.change_status === 'modified')).toHaveLength(1);
        expect(file.sections.some(section => ['added', 'deleted'].includes(section.change_status))).toBe(false);
      }

      const details = [];
      for (const section of file.sections) {
        const response = await page.request.get(`${apiBase}/sections/${section.id}?snapshot_id=${index.snapshot_id}`);
        expect(response.ok()).toBe(true);
        details.push({ section, detail: await response.json() });
      }
      const selected = details.find(({ detail }) => detail.source.includes(fixture.select_contains));
      expect(selected).toBeTruthy();
      const fileLink = page.getByRole('link').filter({ hasText: fixture.path });
      await expect(fileLink).toContainText(`${file.section_count} section`);
      await fileLink.click();
      await expect(page.locator('[data-source-view]')).toHaveCount(1);
      await expect(page.getByRole('button', { name: mode === 'file' ? 'All' : 'Changed', exact: true })).toHaveAttribute('aria-pressed', 'true');
      if (mode !== 'file') {
        await expect(page.getByText(`Showing 1 of ${file.section_count} code sections`, { exact: true })).toBeVisible();
      }
      const target = page.locator(`a[href*="section=${selected.section.id}"]`).first();
      await target.click();
      await expect(page.locator('[data-source-view] code')).toHaveText(selected.detail.source);
      expect(await page.locator('[data-source-view] code').textContent()).toBe(selected.detail.source);

      const kinds = mode === 'file' ? ['Overview', 'Concepts', 'Deep'] : ['Overview', 'Changes', 'Concepts', 'Deep'];
      if (mode === 'file') await expect(page.getByRole('tab', { name: 'Changes', exact: true })).toHaveCount(0);
      for (const kind of kinds) {
        await page.getByRole('tab', { name: kind, exact: true }).click();
        await expect(page.locator('[data-source-view]')).toHaveCount(1);
      }
      expect(await calls(mock)).toBe(0);

      for (const kind of kinds) {
        await page.getByRole('tab', { name: kind, exact: true }).click();
        await page.getByRole('button', { name: `Generate ${kind}`, exact: true }).click();
        await expect(page.getByRole('button', { name: `Regenerate ${kind}`, exact: true })).toBeEnabled();
        await expect(page.locator('[data-source-view]')).toHaveCount(1);
        expect(await page.locator('[data-source-view] code').textContent()).toBe(selected.detail.source);
        if (['Changes', 'Concepts'].includes(kind)) {
          const links = page.getByRole('list', { name: 'Source annotations' }).getByRole('link');
          expect(await links.count()).toBeGreaterThan(0);
          const validTargets = new Set(selected.detail.valid_annotation_regions.map(region => `#source-line-${region.start_line}`));
          for (const link of await links.all()) expect(validTargets.has(await link.getAttribute('href'))).toBe(true);
          await links.first().click();
          await expect(page.locator('[data-source-view] :focus')).toHaveClass('annotated-line');
        }
      }
      expect(await calls(mock)).toBe(kinds.length);
      await page.reload();
      await expect(page.getByRole('button', { name: 'Regenerate Deep', exact: true })).toBeEnabled();
      await expect.poll(() => page.getByRole('button', { name: 'Check saved explanation', exact: true }).isEnabled()).toBe(true);
      const beforeNavigation = requests.length;
      for (const kind of kinds) {
        await page.getByRole('tab', { name: kind, exact: true }).click();
        await expect(page.getByRole('button', { name: `Regenerate ${kind}`, exact: true })).toBeEnabled();
        await expect(page.locator('[data-source-view]')).toHaveCount(1);
      }
      expect(requests).toHaveLength(beforeNavigation);
      expect(await calls(mock)).toBe(kinds.length);
      expect((await fetch(`${mock.origin}/calls`).then(response => response.json())).maxActive).toBe(1);
    });
  }
}
