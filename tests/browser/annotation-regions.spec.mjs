import { test, expect } from '@playwright/test';
import { writeFile } from 'node:fs/promises';
import path from 'node:path';
import { startMock, createRepository, startApp, git } from './workspace-harness.mjs';

const introduced = [
  'fn introduced() -> i32 {',
  '    let values = [',
  '        1,',
  '        2,',
  '    ];',
  '    let total: i32 = values.iter().sum();',
  '    assert!(total > 0);',
  '    total',
  '}',
].join('\n');

for (const mode of ['file', 'diff', 'daemon']) {
  test(`annotation targets ${mode}: distinct statements map to the shared source`, async ({ page }) => {
    const mock = await startMock('valid');
    const repo = await createRepository();
    repo.source += `\n${introduced}\n`;
    await writeFile(path.join(repo.directory, 'src/lib.rs'), repo.source);
    const app = await startApp({ ...repo, modelUrl: mock.url, mode });
    const details = [];
    page.on('response', async response => {
      if (response.url().includes('/ui/v1/sections/') && response.ok()) details.push(await response.json());
    });
    await page.goto(app.url);
    await page.getByRole('link', { name: /src\/lib.rs/ }).click();
    await page.getByRole('link', { name: /introduced.*function/ }).click();
    await expect(page.getByRole('heading', { name: /lib.rs.*introduced/ })).toBeFocused();
    await expect.poll(() => details.find(detail => detail.source === introduced)?.valid_annotation_regions.map(region => [region.start_line, region.end_line]))
      .toEqual([[1, 5], [6, 6], [7, 7], [8, 9]]);
    for (const kind of mode === 'file' ? ['Concepts'] : ['Changes', 'Concepts']) {
      await page.getByRole('tab', { name: kind, exact: true }).click();
      await page.getByRole('button', { name: `Generate ${kind}`, exact: true }).click();
      await expect(page.getByRole('button', { name: `Regenerate ${kind}`, exact: true })).toBeEnabled();
      const links = page.getByRole('list', { name: 'Source annotations' }).getByRole('link');
      await expect(links).toHaveCount(3);
      await expect(links.nth(0)).toHaveAttribute('href', '#source-line-1');
      await expect(links.nth(1)).toHaveAttribute('href', '#source-line-6');
      await expect(links.nth(2)).toHaveAttribute('href', '#source-line-7');
      await links.nth(1).click();
      await expect(page.locator('#source-line-6')).toBeFocused();
      await expect(page.locator('#source-line-6')).toContainText('values.iter().sum()');
      await expect(page.locator('[data-source-view]')).toHaveCount(1);
      await expect(page.locator('#source-line-8')).not.toHaveClass('annotated-line');
      await page.getByRole('tab', { name: 'Overview', exact: true }).click();
      await expect(page.locator('.annotated-line')).toHaveCount(0);
      await page.getByRole('tab', { name: kind, exact: true }).click();
      await expect(links).toHaveCount(3);
    }
    const usage = await fetch(`${mock.origin}/calls`).then(response => response.json());
    expect(usage.calls).toBe(mode === 'file' ? 1 : 2);
    expect(usage.maxActive).toBe(1);
  });
}

for (const mode of ['diff', 'daemon']) {
  test(`annotation targets ${mode}: deleted code uses the same targets in requests and source`, async ({ page }) => {
    const mock = await startMock('valid');
    const repo = await createRepository();
    await writeFile(path.join(repo.directory, 'src/lib.rs'), `${repo.source}\n${introduced}\n`);
    git(repo.directory, 'add', '.');
    git(repo.directory, 'commit', '-qm', 'include removable function');
    const app = await startApp({ ...repo, modelUrl: mock.url, mode });
    await page.goto(app.url);
    await page.getByRole('link', { name: /src\/lib.rs/ }).click();
    await page.getByRole('link', { name: /introduced.*function/ }).click();
    await expect(page.getByRole('heading', { name: 'Source — old revision', exact: true })).toBeVisible();
    for (const kind of ['Changes', 'Concepts']) {
      await page.getByRole('tab', { name: kind, exact: true }).click();
      await page.getByRole('button', { name: `Generate ${kind}`, exact: true }).click();
      await expect(page.getByRole('button', { name: `Regenerate ${kind}`, exact: true })).toBeEnabled();
      const links = page.getByRole('list', { name: 'Source annotations' }).getByRole('link');
      await expect(links).toHaveCount(3);
      await expect(links.nth(1)).toHaveAttribute('href', '#source-line-6');
      await links.nth(1).click();
      await expect(page.locator('#source-line-6')).toBeFocused();
      await expect(page.locator('[data-source-view]')).toHaveCount(1);
    }
  });
}
