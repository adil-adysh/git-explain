import { test, expect } from '@playwright/test';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '../..');
let binary;
const processes = new Set();
const directories = new Set();

test.beforeAll(() => {
  test.setTimeout(180_000);
  const build = spawnSync('cargo', ['build', '--message-format=json'], { cwd: root, encoding: 'utf8', timeout: 180_000 });
  if (build.status !== 0) throw new Error(build.stderr || build.error?.message);
  binary = build.stdout.split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line))
    .find(value => value.reason === 'compiler-artifact' && value.target.name === 'git-explain' && value.executable)?.executable;
  if (!binary) throw new Error('Cargo did not report the git-explain executable');
});

const freePort = () => new Promise((resolve, reject) => {
  const server = net.createServer();
  server.once('error', reject);
  server.listen(0, '127.0.0.1', () => {
    const { port } = server.address();
    server.close(() => resolve(port));
  });
});

const waitForLine = (child, prefix) => new Promise((resolve, reject) => {
  let output = '';
  let errors = '';
  const timer = setTimeout(() => reject(new Error(`Timed out waiting for ${prefix}. Output: ${output}. Errors: ${errors}`)), 15_000);
  child.stderr.on('data', chunk => { errors += chunk.toString(); });
  child.once('error', error => { clearTimeout(timer); reject(error); });
  child.once('exit', code => { clearTimeout(timer); reject(new Error(`Process exited (${code}): ${errors}`)); });
  child.stdout.on('data', chunk => {
    output += chunk.toString();
    const line = output.split(/\r?\n/).slice(0, -1).find(value => value.startsWith(prefix));
    if (line) {
      clearTimeout(timer);
      resolve(line.slice(prefix.length));
    }
  });
});

const startMock = async scenario => {
  const port = await freePort();
  const child = spawn(process.execPath, [path.join(root, 'scripts/mock-model-server.mjs'), '--port', String(port), '--scenario', scenario], { stdio: ['ignore', 'pipe', 'pipe'] });
  processes.add(child);
  const url = await waitForLine(child, 'MOCK_MODEL_URL=');
  return { child, url, origin: url.replace(/\/v1$/, '') };
};

const git = (cwd, ...args) => {
  const result = spawnSync('git', args, { cwd, encoding: 'utf8' });
  if (result.status !== 0) throw new Error(result.stderr);
};

const createRepository = async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'git-explain-browser-'));
  directories.add(directory);
  git(directory, 'init', '-q');
  git(directory, 'config', 'user.email', 'browser@example.test');
  git(directory, 'config', 'user.name', 'Browser Test');
  await mkdir(path.join(directory, 'src'));
  const source = [
    'fn calculate() -> i32 {',
    '    let first = 1;',
    '    let a = first + 1;',
    '    let b = a + 1;',
    '    let c = b + 1;',
    '    let d = c + 1;',
    '    let e = d + 1;',
    '    let f = e + 1;',
    '    let g = f + 1;',
    '    let h = g + 1;',
    '    let i = h + 1;',
    '    let j = i + 1;',
    '    let last = j + 1;',
    '    first + last',
    '}',
    '',
  ].join('\n');
  await writeFile(path.join(directory, 'src/lib.rs'), source);
  git(directory, 'add', '.');
  git(directory, 'commit', '-qm', 'base');
  return { directory, source };
};

const startApp = async ({ directory, modelUrl, mode, source, cache = false, reuseConfig = false }) => {
  const port = await freePort();
  const config = path.join(directory, 'config.toml');
  if (!reuseConfig) await writeFile(config, `[model]\nprofile = "mock"\n\n[profiles.mock]\nprovider = "openai_compatible"\npreset = "llama_cpp"\nbase_url = "${modelUrl}"\nmodel = "mock-model"\n\n[server]\nhost = "127.0.0.1"\nport = ${port}\nopen_browser = false\n\n[cache]\nenabled = ${cache}\n\n[explanation]\nmax_annotations = 3\n`);
  if (mode === 'diff' || mode === 'daemon') {
    const changed = source.replace('let first = 1;', 'let first = 10;').replace('let last = j + 1;', 'let last = j + 10;');
    await writeFile(path.join(directory, 'src/lib.rs'), changed);
  }
  const args = mode === 'daemon' ? ['daemon', 'run'] : ['--profile', 'mock', '--port', String(port), '--direct', mode];
  if (mode === 'file') args.push('--', 'src');
  const child = spawn(binary, args, {
    cwd: directory,
    env: { ...process.env, GIT_EXPLAIN_USER_CONFIG: config, APPDATA: directory, XDG_CONFIG_HOME: directory,
      GIT_EXPLAIN_DAEMON_DIR: path.join(directory, 'daemon'), GIT_EXPLAIN_DAEMON_PORT: String(port) },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  processes.add(child);
  if (mode === 'daemon') {
    await waitForLine(child, 'git-explain daemon listening on ');
    const origin = `http://127.0.0.1:${port}`;
    const metadata = JSON.parse(await readFile(path.join(directory, 'daemon/daemon.json'), 'utf8'));
    const response = await fetch(`${origin}/api/repos/open`, {
      method: 'POST', headers: { 'Content-Type': 'application/json', 'x-git-explain-control-token': metadata.control_token },
      body: JSON.stringify({ repo_root: directory, profile: 'mock' }),
    });
    const opened = await response.json();
    expect(response.ok).toBe(true);
    expect(opened.ok).toBe(true);
    return { child, origin, url: `${origin}/sessions/${opened.session_id}`, session: opened.session_id, controlToken: metadata.control_token };
  }
  await waitForLine(child, 'git explain: ');
  return { child, url: `http://127.0.0.1:${port}` };
};

const openConcepts = async (page, appUrl) => {
  await page.goto(appUrl);
  const article = page.locator('article').filter({ has: page.getByRole('heading', { name: 'calculate', exact: true }) });
  await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
  return article;
};

const generate = async article => {
  await article.locator('[data-generate]').click();
  await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
};

test.afterEach(async () => {
  await Promise.all([...processes].map(child => new Promise(resolve => {
    if (child.exitCode !== null || child.signalCode !== null) return resolve();
    child.once('exit', resolve);
    child.kill();
  })));
  processes.clear();
  for (const directory of directories) await rm(directory, { recursive: true, force: true, maxRetries: 5 });
  directories.clear();
});

test('file mode normalizes duplicate annotations and renders source once', async ({ page }) => {
  const mock = await startMock('duplicates');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(0);
  await generate(article);
  await expect(article.locator('.annotation')).toHaveCount(2);
  const sourceMatches = await article.evaluate(element => {
    const expected = element.querySelector('.source-text').value.split('\n');
    const actual = [...element.querySelectorAll('.source-code code')].flatMap(node => node.textContent.split('\n'));
    return JSON.stringify(expected) === JSON.stringify(actual);
  });
  expect(sourceMatches).toBe(true);
});

for (const scenario of ['malformed_then_valid', 'wrong_kind_then_valid', 'invalid_region_then_valid']) {
  test(`file mode retries ${scenario.replaceAll('_', ' ')}`, async ({ page }) => {
    const mock = await startMock(scenario);
    const repo = await createRepository();
    const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
    const article = await openConcepts(page, app.url);
    await generate(article);
    await expect(article.locator('.annotation')).toHaveCount(1);
    expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(2);
  });
}

test('diff mode retries missing changed-region coverage and renders every source line once', async ({ page }) => {
  const mock = await startMock('missing_coverage_then_valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'diff' });
  const article = await openConcepts(page, app.url);
  await generate(article);
  await expect(article.locator('.annotation')).toHaveCount(2);
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(2);
  expect(await article.evaluate(element => {
    const expected = element.querySelector('.source-text').value.split('\n');
    const actual = [...element.querySelectorAll('.source-code code')].flatMap(node => node.textContent.split('\n'));
    return JSON.stringify(expected) === JSON.stringify(actual);
  })).toBe(true);
});

test('delayed generation keeps progress visible until the response arrives', async ({ page }) => {
  const mock = await startMock('delayed_valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  await article.locator('[data-generate]').click();
  await expect(article.locator('[data-generate]')).toBeDisabled();
  await expect(article.locator('[data-generate]')).toHaveText('Generating…');
  await expect(article.locator('.annotation')).toHaveCount(0);
  await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
  await expect(article.locator('.annotation')).toHaveCount(1);
});

test('retry failure is stable and leaves source visible', async ({ page }) => {
  const mock = await startMock('always_malformed');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  await article.locator('[data-generate]').click();
  await expect(article.locator('.layer-summary')).toContainText('could not generate an explanation');
  await expect(article.locator('.annotation')).toHaveCount(0);
  await expect(article.locator('.source-code')).toBeVisible();
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(2);
});

// Hold transport until the test has navigated, avoiding timing-dependent races.
const holdRequest = async (page, pattern) => {
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  await page.route(pattern, async route => { await gate; await route.continue(); });
  return release;
};

test('completion preserves navigation and prevents duplicate generation', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  const release = await holdRequest(page, '**/concepts');
  const completed = page.waitForResponse(response => response.url().endsWith('/concepts'));
  await article.locator('[data-generate]').click();
  await expect(article.locator('[data-generate]')).toBeDisabled();
  await article.getByRole('tab', { name: 'Overview', exact: true }).click();
  await expect(article.locator('[data-generate]')).toBeDisabled();
  release();
  await completed;
  await expect(article.locator('[data-generate]')).toBeEnabled();
  await expect(article.getByRole('tab', { name: 'Overview', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(article.locator('.annotation')).toHaveCount(0);
  await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await expect(article.locator('.annotation')).toHaveCount(1);
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(1);
});

test('hide explanations survives redraw and does not reveal inactive Deep', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  await page.getByRole('button', { name: 'Hide explanations', exact: true }).click();
  await generate(article);
  await expect(article.locator('.annotation')).toBeHidden();
  await page.getByRole('button', { name: 'Show explanations', exact: true }).click();
  await expect(article.locator('.annotation')).toBeVisible();
  await expect(article.locator('.deep-section')).toBeHidden();
});

for (const mode of ['file', 'diff']) {
  test(`${mode} tabs restore independent results without requests and invalidate only dependent Deep`, async ({ page }) => {
    const mock = await startMock('valid');
    const repo = await createRepository();
    const app = await startApp({ ...repo, modelUrl: mock.url, mode });
    const article = await openConcepts(page, app.url);
    const requests = [];
    page.on('request', request => { if (request.method() === 'POST' && !request.url().includes('/learning')) requests.push(request.url()); });
    const names = mode === 'file' ? ['Overview', 'Concepts', 'Explain this code in depth'] : ['Overview', 'Changes', 'Concepts', 'Explain this code in depth'];
    for (const name of names) {
      await article.getByRole('tab', { name, exact: true }).click();
      await generate(article);
    }
    expect(requests.map(url => new URL(url).pathname.split('/').at(-1))).toEqual(mode === 'file' ? ['explain', 'concepts', 'deep'] : ['overview', 'explain', 'concepts', 'deep']);
    const generated = requests.length;
    for (const name of names) {
      await article.getByRole('tab', { name, exact: true }).click();
      await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
    }
    expect(requests).toHaveLength(generated);
    await expect(article.locator('.deep')).toContainText('Understand this code.');
    await expect(article.locator('.deep')).toContainText('Walkthrough');
    await expect(article.locator('.layer-summary')).not.toHaveText('Detailed teaching for this code.');
    await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
    await generate(article);
    await article.getByRole('tab', { name: 'Explain this code in depth', exact: true }).click();
    await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
    await article.getByRole('tab', { name: mode === 'file' ? 'Overview' : 'Changes', exact: true }).click();
    await generate(article);
    await article.getByRole('tab', { name: 'Explain this code in depth', exact: true }).click();
    await expect(article.locator('[data-generate]')).toHaveText('Generate selected explanation');
    await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
    await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
    await page.reload();
    await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
    await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
    expect(requests.map(url => new URL(url).pathname.split('/').slice(-1)[0])).toContain('regenerate');
  });
}

test('failed background request stays with its view and can be retried', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  await page.route('**/concepts', async route => { await gate; await route.abort(); });
  await article.locator('[data-generate]').click();
  await article.getByRole('tab', { name: 'Overview', exact: true }).click();
  release();
  await expect(article.locator('[data-generate]')).toBeEnabled();
  await expect(article.locator('.layer-summary')).toContainText('not been generated');
  await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await expect(article.locator('.layer-summary')).toContainText('try again');
  await page.unroute('**/concepts');
  await generate(article);
  await expect(article.locator('.annotation')).toHaveCount(1);
});

test('source and diff controls preserve a single source view when changing explanations', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'diff' });
  const article = await openConcepts(page, app.url);
  await generate(article);
  await article.getByRole('button', { name: 'Read code as text', exact: true }).click();
  await expect(article.locator('.source-text')).toBeVisible();
  await expect(article.locator('.source-code')).toBeHidden();
  await article.getByRole('tab', { name: 'Changes', exact: true }).click();
  await generate(article);
  await expect(article.locator('.source-text')).toBeVisible();
  await expect(article.locator('.source-code')).toBeHidden();
  await article.getByRole('button', { name: 'Show rendered code', exact: true }).click();
  await expect(article.locator('.source-text')).toBeHidden();
  await expect(article.locator('.source-code')).toBeVisible();
  await expect(article.locator('.source-code')).toHaveCount(1);
  await article.getByRole('button', { name: 'Show Git diff', exact: true }).click();
  await expect(article.getByRole('heading', { name: 'Git diff', exact: true })).toBeVisible();
  await article.getByRole('button', { name: 'Hide Git diff', exact: true }).click();
  await expect(article.getByRole('heading', { name: 'Git diff', exact: true })).toBeHidden();
});

test('reader controls fit a narrow screen', async ({ page }, testInfo) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  await page.setViewportSize({ width: 390, height: 844 });
  const article = await openConcepts(page, app.url);
  await generate(article);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await expect(page.getByRole('searchbox')).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('reader-mobile.png'), fullPage: true });
});

test('filtering a larger file selection preserves generated results', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  for (let index = 0; index < 40; index++) {
    await writeFile(path.join(repo.directory, `src/module_${index}.rs`), `fn entry_${index}() {}\nfn helper_${index}() {}\n`);
  }
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  await generate(article);
  const search = page.getByRole('searchbox', { name: 'Filter files and code sections' });
  await expect(page.locator('article:visible')).toHaveCount(81);
  await search.fill('entry_39');
  await expect(page.locator('article:visible')).toHaveCount(1);
  await expect(page.getByRole('heading', { name: 'entry_39', exact: true })).toBeVisible();
  await search.fill('src/module_39.rs');
  await expect(page.locator('article:visible')).toHaveCount(2);
  await search.fill('calculate');
  await expect(article.locator('.annotation')).toHaveCount(1);
  await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(1);
});

test('keyboard tabs navigate without generating and expose their panel', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  const concepts = article.getByRole('tab', { name: 'Concepts', exact: true });
  await concepts.focus();
  await page.keyboard.press('ArrowLeft');
  const overview = article.getByRole('tab', { name: 'Overview', exact: true });
  await expect(overview).toBeFocused();
  await expect(overview).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('End');
  await expect(article.getByRole('tab', { name: 'Explain this code in depth', exact: true })).toBeFocused();
  await page.keyboard.press('Home');
  await expect(overview).toBeFocused();
  const panel = article.getByRole('tabpanel');
  await expect(panel).toHaveAttribute('aria-labelledby', await overview.getAttribute('id'));
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(0);
});

for (const mode of ['file', 'diff']) {
  test(`${mode} filters file paths and code sections locally with an empty state`, async ({ page }) => {
    const mock = await startMock('valid');
    const repo = await createRepository();
    await writeFile(path.join(repo.directory, 'src/other.rs'), 'fn secondary() { let value = 1; }\n');
    git(repo.directory, 'add', '.');
    git(repo.directory, 'commit', '-qm', 'second file');
    if (mode === 'diff') await writeFile(path.join(repo.directory, 'src/other.rs'), 'fn secondary() { let value = 2; }\n');
    const app = await startApp({ ...repo, modelUrl: mock.url, mode });
    await page.goto(app.url);
    await expect(page.getByRole('heading', { name: 'secondary', exact: true })).toBeVisible();
    const search = page.getByRole('searchbox', { name: 'Filter files and code sections' });
    await search.fill('CALCULATE');
    await expect(page.getByRole('heading', { name: 'calculate', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'secondary', exact: true })).toBeHidden();
    await expect(page.getByRole('link', { name: /other.rs/ })).toBeHidden();
    await search.fill('missing-code');
    await expect(page.getByText('No files or code sections match your filter.')).toBeVisible();
    await expect(page.locator('article:visible')).toHaveCount(0);
    await search.fill('src/lib.rs');
    await expect(page.getByRole('heading', { name: 'calculate', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Clear filter' }).click();
    await expect(search).toHaveValue('');
    await expect(search).toBeFocused();
    await expect(page.getByRole('heading', { name: 'secondary', exact: true })).toBeVisible();
    expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(0);
  });
}

test('stale response offers reload and errors remain scoped to their view', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'diff' });
  const article = await openConcepts(page, app.url);
  await page.route('**/concepts', route => route.fulfill({ json: { ok: false, stale: true, error: 'old source' } }));
  await article.locator('[data-generate]').click();
  await expect(article.locator('.layer-summary')).toContainText('Reload');
  await expect(article.getByRole('button', { name: 'Reload Git changes' })).toBeVisible();
  await article.getByRole('tab', { name: 'Overview', exact: true }).click();
  await expect(article.locator('.layer-summary')).not.toContainText('old source');
  await expect(article.locator('.source-code')).toBeVisible();
});

for (const mode of ['file', 'diff']) {
  test(`${mode} server serializes generation across code sections`, async ({ page }) => {
    const mock = await startMock('delayed_valid');
    const repo = await createRepository();
    await writeFile(path.join(repo.directory, 'src/other.rs'), 'fn secondary() { let value = 1; }\n');
    git(repo.directory, 'add', '.');
    git(repo.directory, 'commit', '-qm', 'second file');
    if (mode === 'diff') await writeFile(path.join(repo.directory, 'src/other.rs'), 'fn secondary() { let value = 2; }\n');
    const app = await startApp({ ...repo, modelUrl: mock.url, mode });
    await page.goto(app.url);
    const results = await page.evaluate(() => Promise.all([...document.querySelectorAll('[data-layer=concepts]')].map(async tab => {
      const response = await fetch(tab.dataset.endpoint, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(tab.dataset.generation ? { generation: Number(tab.dataset.generation) } : {}) });
      return response.json();
    })));
    expect(results).toHaveLength(2);
    expect(results.every(value => value.ok)).toBe(true);
    const activity = await fetch(`${mock.origin}/calls`).then(r => r.json());
    expect(activity.calls).toBe(2);
    expect(activity.maxActive).toBe(1);
  });
}

test('daemon learning saves and navigation do not generate; concurrent sections retain policies', async ({ page }) => {
  const mock = await startMock('delayed_valid');
  const repo = await createRepository();
  await writeFile(path.join(repo.directory, 'src/other.rs'), 'fn secondary() -> i32 { 1 }\n');
  git(repo.directory, 'add', '.');
  git(repo.directory, 'commit', '-qm', 'second file');
  await writeFile(path.join(repo.directory, 'src/other.rs'), 'fn secondary() -> i32 { 2 }\n');
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'daemon' });
  const article = await openConcepts(page, app.url);
  await page.locator('#learning-settings > summary').click();
  await expect(page.locator('#learning-profile')).toBeEnabled();
  await page.getByText('Customize saved preferences', { exact: true }).click();
  await page.locator('#learning-name').fill('Daemon reader');
  await page.locator('#learning-editor [data-preference="detail"]').selectOption('detailed');
  await page.locator('#learning-save-as').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');
  await page.locator('#learning-default').click();
  const endpoint = `${app.origin}/api/sessions/${app.session}/learning`;
  await expect.poll(async () => (await page.request.get(endpoint).then(r => r.json())).config.profile).toBe('Daemon reader');
  const settings = await page.request.get(endpoint).then(r => r.json());
  expect(settings.config.profiles['Daemon reader'].preferences.detail).toBe('detailed');
  const denied = await page.request.put(endpoint, { data: { revision: settings.revision, config: settings.config } });
  expect(denied.status()).toBe(403);
  await article.getByRole('tab', { name: 'Changes', exact: true }).click();
  await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(0);
  const results = await page.evaluate(async () => Promise.all([...document.querySelectorAll('[data-layer="concepts"]')].map(async (tab, index) => {
    const focus = index === 0 ? 'syntax' : 'reasoning';
    const response = await fetch(tab.dataset.endpoint, { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ generation: Number(tab.dataset.generation), learning: { profile: 'Daemon reader', overrides: { focus } } }) });
    return { focus, status: response.status, result: await response.json() };
  })));
  expect(results).toHaveLength(2);
  for (const { focus, status, result } of results) {
    expect(status).toBe(200);
    expect(result.ok).toBe(true);
    expect(result.policy.focus).toBe(focus);
    expect(result.policy.detail).toBe('detailed');
  }
  const activity = await fetch(`${mock.origin}/calls`).then(r => r.json());
  expect(activity.calls).toBe(2);
  expect(activity.maxActive).toBe(1);
});

test('restart restores saved profiles and policy-specific Deep identities without persisting temporary preferences', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file', cache: true });
  let article = await openConcepts(page, app.url);
  await page.locator('#learning-settings > summary').click();
  await expect(page.locator('#learning-profile')).toBeEnabled();
  await page.getByText('Customize saved preferences', { exact: true }).click();
  await page.locator('#learning-name').fill('Persistent reader');
  await page.locator('#learning-editor [data-preference="focus"]').selectOption('syntax');
  await page.locator('#learning-save-as').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');
  await page.locator('#learning-default').click();
  await expect.poll(async () => (await page.request.get(`${app.url}/api/learning`).then(r => r.json())).config.profile).toBe('Persistent reader');
  const run = async kind => {
    await article.locator(`[data-layer="${kind}"]`).click();
    await expect(article.locator('[data-generate]')).toBeEnabled();
    const endpoint = await article.locator(`[data-layer="${kind}"]`).getAttribute('data-endpoint');
    const pending = page.waitForResponse(response => response.request().method() === 'POST' && new URL(response.url()).pathname === endpoint);
    await generate(article);
    const response = await pending;
    expect(response.ok()).toBe(true);
    const result = await response.json();
    expect(result.ok).toBe(true);
    return result;
  };
  const overviewA = await run('overview');
  const deepA = await run('deep');
  expect(deepA.prerequisite_id).toBe(overviewA.result_id);
  const focus = page.locator('#learning-settings fieldset [data-preference="focus"]');
  await focus.selectOption('execution');
  await expect(article.locator('.policy-status')).toContainText('Pending settings: brief · execution · balanced');
  const overviewB = await run('overview');
  const deepB = await run('deep');
  expect(deepB.prerequisite_id).toBe(overviewB.result_id);
  expect(overviewA.policy_id).not.toBe(overviewB.policy_id);
  const calls = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  expect(calls).toBe(4);
  const exited = new Promise(resolve => app.child.once('exit', resolve));
  app.child.kill();
  await exited;
  processes.delete(app.child);

  const restarted = await startApp({ ...repo, modelUrl: mock.url, mode: 'file', cache: true, reuseConfig: true });
  article = await openConcepts(page, restarted.url);
  await expect(page.locator('#learning-profile')).toHaveValue('Persistent reader');
  await expect(focus).toHaveValue('');
  const lookup = async (kind, expected) => {
    await article.locator(`[data-layer="${kind}"]`).click();
    await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
    const restored = await page.request.post(`${restarted.url}/api/learning/results`, {
      data: { language: 'rust', unit_id: await article.getAttribute('data-unit-id'), kind,
        learning: { profile: 'Persistent reader', session: expected.policy.focus === 'execution' ? { focus: 'execution' } : {} } },
    }).then(r => r.json());
    expect(restored.ok).toBe(true);
    expect(restored.policy_id).toBe(expected.policy_id);
    expect(restored.result_id).toBe(expected.result_id);
    expect(restored.prerequisite_id).toBe(expected.prerequisite_id);
    if (kind === 'overview') await expect(article.locator('.layer-summary')).toHaveText(expected.overview);
    else {
      await expect(article.locator('.deep-section')).toBeVisible();
      await expect(article.locator('.deep')).toContainText(expected.teaching.purpose);
      for (const step of expected.teaching.walkthrough) await expect(article.locator('.deep')).toContainText(step);
    }
  };
  await lookup('overview', overviewA);
  await lookup('deep', deepA);
  await page.locator('#learning-settings > summary').click();
  await focus.selectOption('execution');
  await lookup('overview', overviewB);
  await lookup('deep', deepB);
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(calls);
});

test('daemon prerequisite replacement cannot mix Deep variants during queued generation', async ({ page }) => {
  const mock = await startMock('controlled_valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'daemon' });
  const article = await openConcepts(page, app.url);
  const generation = Number(await article.locator('[data-layer="changes"]').getAttribute('data-generation'));
  const unit = await article.getAttribute('data-unit-id');
  const endpoint = async (kind, regenerate = false) => app.origin + await article.locator(`[data-layer="${kind}"]`).getAttribute(regenerate ? 'data-regenerate-endpoint' : 'data-endpoint');
  const changesUrl = await endpoint('changes'), regenUrl = await endpoint('changes', true), deepUrl = await endpoint('deep');
  const selection = focus => ({ overrides: { focus } });
  const generateAt = (url, focus) => page.request.post(url, { data: { generation, learning: selection(focus) } }).then(r => r.json());
  const oldA = await generateAt(changesUrl, 'syntax');
  const changesB = await generateAt(changesUrl, 'reasoning');
  const deepB = await generateAt(deepUrl, 'reasoning');
  expect(oldA.ok).toBe(true);
  expect(changesB.ok).toBe(true);
  expect(deepB.prerequisite_id).toBe(changesB.result_id);
  await fetch(`${mock.origin}/hold`, { method: 'POST' });
  const regenerating = generateAt(regenUrl, 'syntax');
  await expect.poll(async () => (await fetch(`${mock.origin}/calls`).then(r => r.json())).held).toBe(1);
  const pendingDeep = generateAt(deepUrl, 'syntax');
  // A real model request is held while Deep is submitted; no timing-only sleep.
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).active).toBe(1);
  await fetch(`${mock.origin}/release`, { method: 'POST' });
  const newA = await regenerating, deepA = await pendingDeep;
  expect(newA.ok).toBe(true);
  expect(newA.result_id).not.toBe(oldA.result_id);
  if (deepA.ok) expect(deepA.prerequisite_id).toBe(newA.result_id);
  else {
    expect(deepA.code).toBe('prerequisite_changed');
    expect(deepA.retryable).toBe(true);
  }
  const lookup = focus => page.request.post(`${app.origin}/api/sessions/${app.session}/learning/results`, {
    data: { language: 'rust', unit_id: unit, kind: 'deep', learning: selection(focus) },
  }).then(r => r.json());
  const restoredA = await lookup('syntax');
  if (deepA.ok) expect(restoredA.prerequisite_id).toBe(newA.result_id);
  else {
    expect(restoredA.result).toBeNull();
    const retriedA = await generateAt(deepUrl, 'syntax');
    expect(retriedA.ok).toBe(true);
    expect(retriedA.prerequisite_id).toBe(newA.result_id);
  }
  expect((await lookup('reasoning')).result_id).toBe(deepB.result_id);
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).maxActive).toBe(1);
});

test('daemon discards a held generation after source refresh replaces its session', async ({ page }) => {
  const mock = await startMock('controlled_valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'daemon' });
  const article = await openConcepts(page, app.url);
  const endpoint = await article.locator('[data-layer="concepts"]').getAttribute('data-endpoint');
  await fetch(`${mock.origin}/hold`, { method: 'POST' });
  const generating = page.waitForResponse(response => response.request().method() === 'POST' && new URL(response.url()).pathname === endpoint);
  await article.locator('[data-generate]').click();
  await expect.poll(async () => (await fetch(`${mock.origin}/calls`).then(r => r.json())).held).toBe(1);
  await writeFile(path.join(repo.directory, 'src/lib.rs'), repo.source.replace('let first = 1;', 'let first = 20;'));
  const refreshed = await page.request.post(`${app.origin}/api/repos/refresh`, {
    headers: { 'x-git-explain-control-token': app.controlToken }, data: { repo_root: repo.directory },
  }).then(r => r.json());
  expect(refreshed.ok).toBe(true);
  expect(refreshed.changed).toBe(true);
  await fetch(`${mock.origin}/release`, { method: 'POST' });
  const stale = await (await generating).json();
  expect(stale.ok).toBe(false);
  expect(stale.stale).toBe(true);
  expect(stale.code).toBe('session_stale');
  await expect(article.locator('.layer-summary')).toContainText('Reload Git changes and try again.');
  const reload = article.locator('[data-reload]');
  await expect(reload).toBeVisible();
  await Promise.all([page.waitForEvent('load'), reload.click()]);
  const currentArticle = page.locator('article[data-unit-id]').first();
  await expect(currentArticle.locator('[data-reload]')).toBeHidden();
  await expect(currentArticle.locator('.layer-summary')).not.toContainText('Reload Git changes and try again.');
  const current = await page.request.post(`${app.origin}/api/sessions/${app.session}/learning/results`, {
    data: { language: 'rust', unit_id: await currentArticle.getAttribute('data-unit-id'), kind: 'concepts' },
  }).then(r => r.json());
  expect(current.ok).toBe(true);
  expect(current.result).toBeNull();
  const activity = await fetch(`${mock.origin}/calls`).then(r => r.json());
  expect(activity.calls).toBe(1);
  expect(activity.maxActive).toBe(1);
});

test('learning controls change pending settings without starting a model request', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  await page.goto(app.url);
  await page.locator('#learning-settings > summary').click();
  await expect(page.locator('#learning-profile')).toBeEnabled();
  await page.locator('#learning-settings fieldset [data-preference="focus"]').selectOption('execution');
  const article = page.locator('article[data-unit-id]').first();
  await article.locator('[data-layer="concepts"]').click();
  await article.locator('.view-learning summary').click();
  await article.locator('[data-preference="detail"]').selectOption('detailed');
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(0);
  await expect(article.locator('.policy-status')).toContainText('Pending settings');
  await page.waitForLoadState('networkidle');
  const navigationRequests = [];
  page.on('request', request => navigationRequests.push(request.url()));
  await article.locator('[data-layer="overview"]').click();
  await article.locator('[data-layer="deep"]').click();
  expect(navigationRequests).toEqual([]);
});

test('saved learning profiles and editable feedback require explicit saving', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  await page.locator('#learning-settings > summary').click();
  await expect(page.locator('#learning-profile')).toBeEnabled();
  await page.getByText('Customize saved preferences', {exact:true}).click();
  await page.locator('#learning-name').fill('Learning Rust');
  await page.locator('#learning-editor [data-preference="language_help"]').selectOption('explain');
  await page.locator('[data-list="known_languages"]').fill('Python');
  await page.locator('#learning-save-as').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(0);
  await generate(article);
  await article.locator('[data-feedback="known"]').first().click();
  let settings = await page.request.get(`${app.url}/api/learning`).then(response => response.json());
  expect(settings.config.profiles['Learning Rust'].preferences.known_concepts || []).toEqual([]);
  await article.locator('.view-learning > summary').click();
  await article.locator('.feedback-term').fill('Rust lock guards');
  await article.locator('[data-save-feedback]').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');
  await expect.poll(async () => {
    settings = await page.request.get(`${app.url}/api/learning`).then(response => response.json());
    return settings.config.profiles['Learning Rust'].preferences.known_concepts;
  }).toEqual(['Rust lock guards']);
});

test('learning variants restore independently and preserve settings captured at Generate', async ({ page }) => {
  const mock = await startMock('delayed_valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  await page.locator('#learning-settings > summary').click();
  await expect(page.locator('#learning-profile')).toBeEnabled();
  const focus = page.locator('#learning-settings fieldset [data-preference="focus"]');
  await focus.selectOption('execution');
  await expect(article.locator('.policy-status')).toContainText('execution');
  const request = page.waitForRequest(request => request.url().endsWith('/concepts') && request.method() === 'POST');
  await article.locator('[data-generate]').click();
  expect((await request).postDataJSON().learning.session.focus).toBe('execution');
  await focus.selectOption('syntax');
  await expect(article.locator('[data-generate]')).toBeEnabled();
  await expect(article.locator('[data-generate]')).toHaveText('Generate selected explanation');
  await focus.selectOption('execution');
  await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
  const calls = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  await focus.selectOption('syntax');
  await expect(article.locator('[data-generate]')).toHaveText('Generate selected explanation');
  await expect(article.locator('.policy-status')).toContainText('Pending settings:');
  await expect(article.locator('.policy-status')).toContainText('syntax');
  await expect(article.locator('.policy-status')).toContainText('Displayed settings:');
  await expect(article.locator('.policy-status')).toContainText('execution');
  await expect(article.locator('.policy-status')).toContainText('Showing an explanation generated with different settings.');
  await focus.selectOption('execution');
  await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
  await expect(article.locator('.policy-status')).toContainText('Showing explanation generated with these settings.');
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(calls);
});

test('settings-used: displays actual settings used by existing explanation when pending selection differs', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  await page.locator('#learning-settings > summary').click();
  await expect(page.locator('#learning-profile')).toBeEnabled();

  const languageHelp = page.locator('#learning-settings fieldset [data-preference="language_help"]');
  const focus = page.locator('#learning-settings fieldset [data-preference="focus"]');
  const detail = page.locator('#learning-settings fieldset [data-preference="detail"]');

  // Policy A: explain / syntax / concise
  await languageHelp.selectOption('explain');
  await focus.selectOption('syntax');
  await detail.selectOption('concise');

  await expect(article.locator('.policy-status')).toContainText('explain · syntax · concise');
  await generate(article);
  await expect(article.locator('.policy-status')).toContainText('Showing explanation generated with these settings.');

  const originalOverview = await article.locator('.layer-summary').textContent();
  expect(originalOverview).not.toBe('This explanation has not been generated.');
  const callsAfterA = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;

  // Change pending to Policy B: skip / reasoning / detailed
  await languageHelp.selectOption('skip');
  await focus.selectOption('reasoning');
  await detail.selectOption('detailed');

  // Assert original readable result remains and .policy-status literally reports both correctly labeled triples
  await expect(article.locator('.policy-status')).toContainText('Pending settings: skip · reasoning · detailed');
  await expect(article.locator('.policy-status')).toContainText('Displayed settings: explain · syntax · concise');
  await expect(article.locator('.policy-status')).toContainText('Showing an explanation generated with different settings.');
  expect(await article.locator('.layer-summary').textContent()).toBe(originalOverview);

  // Assert mock calls unchanged by preference changes and navigation
  let calls = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  expect(calls).toBe(callsAfterA);

  await article.getByRole('tab', { name: 'Overview', exact: true }).click();
  await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
  calls = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  expect(calls).toBe(callsAfterA);
  await expect(article.locator('.policy-status')).toContainText('Pending settings: skip · reasoning · detailed');
  await expect(article.locator('.policy-status')).toContainText('Displayed settings: explain · syntax · concise');
  expect(await article.locator('.layer-summary').textContent()).toBe(originalOverview);

  // Generate B then switch back to A and verify displayed settings/result restores, without additional generation
  await generate(article);
  await expect(article.locator('.policy-status')).toContainText('Pending settings: skip · reasoning · detailed');
  await expect(article.locator('.policy-status')).toContainText('Showing explanation generated with these settings.');
  const callsAfterB = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  expect(callsAfterB).toBeGreaterThan(callsAfterA);

  await languageHelp.selectOption('explain');
  await focus.selectOption('syntax');
  await detail.selectOption('concise');

  await expect(article.locator('.policy-status')).toContainText('Pending settings: explain · syntax · concise');
  await expect(article.locator('.policy-status')).toContainText('Showing explanation generated with these settings.');
  await expect(article.locator('.layer-summary')).toHaveText(originalOverview);
  await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
  const callsFinal = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  expect(callsFinal).toBe(callsAfterB);
});

test('view-specific learning settings follow the selected tab without network requests', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const article = await openConcepts(page, app.url);
  await article.locator('.view-learning > summary').click();
  await article.locator('[data-preference="detail"]').selectOption('detailed');
  await expect(article.locator('.policy-status')).toContainText('detailed');
  await generate(article);
  await page.waitForLoadState('networkidle');
  const requests = [];
  page.on('request', request => requests.push(request.url()));
  await article.getByRole('tab', { name: 'Overview', exact: true }).click();
  await expect(article.locator('.policy-status')).toContainText('brief · balanced · balanced');
  expect(requests).toEqual([]);
  await generate(article);
  await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await expect(article.locator('.policy-status')).toContainText('detailed');
  await expect(article.locator('[data-generate]')).toHaveText('Regenerate explanation');
});

test('feedback-moves concepts between known and learning lists preserving unrelated entries', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  await page.goto(app.url);
  await expect(page.locator('#learning-profile')).toBeEnabled();

  const profileName = 'Feedback Movement';
  const seeded = await page.evaluate(async name => {
    const current = await (await fetch('/api/learning')).json();
    current.config.profile = name;
    current.config.profiles[name] = {
      preferences: {
        known_concepts: ['Already known', 'Unrelated known'],
        learning_concepts: ['Concept 1', 'Still learning', 'Unrelated learning'],
      },
      languages: {},
    };
    return (await fetch('/api/learning', {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
        'x-git-explain-learning-token': current.token,
      },
      body: JSON.stringify({ revision: current.revision, config: current.config }),
    })).json();
  }, profileName);
  expect(seeded.ok).toBe(true);

  await page.reload();
  await expect(page.locator('#learning-profile')).toHaveValue(profileName);

  const article = page.locator('article[data-unit-id]').first();
  await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
  await generate(article);

  const callsAfterGenerate = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  expect(callsAfterGenerate).toBe(1);

  // Direction 1: Move Concept 1 from learning to known
  await article.locator('[data-feedback="known"]').first().click();
  await article.locator('.view-learning > summary').click();
  await expect(article.locator('.feedback-term')).toHaveValue('Concept 1');

  // Before Save, assert saved settings unchanged
  let settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  let prefs = settings.config.profiles[profileName].preferences;
  expect(prefs.known_concepts).toEqual(['Already known', 'Unrelated known']);
  expect(prefs.learning_concepts).toEqual(['Concept 1', 'Still learning', 'Unrelated learning']);

  // Explicit save
  await article.locator('[data-save-feedback]').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');

  // After Save, assert selected concept in only requested list and absent from opposite list,
  // and unrelated entries are preserved
  await expect.poll(async () => {
    settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
    const p = settings.config.profiles[profileName].preferences;
    return { known: p.known_concepts, learning: p.learning_concepts };
  }).toEqual({
    known: ['Already known', 'Unrelated known', 'Concept 1'],
    learning: ['Still learning', 'Unrelated learning'],
  });

  // Assert feedback and saving caused no additional model calls
  let calls = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  expect(calls).toBe(callsAfterGenerate);

  // Direction 2: Move Concept 1 from known to learning, exercising editable label behavior
  await article.locator('[data-feedback="learning"]').first().click();
  await expect(article.locator('.feedback-term')).toHaveValue('Concept 1');
  // Exercise editable label behavior with case change
  await article.locator('.feedback-term').fill('concept 1');

  // Before Save, assert saved settings unchanged
  settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  prefs = settings.config.profiles[profileName].preferences;
  expect(prefs.known_concepts).toEqual(['Already known', 'Unrelated known', 'Concept 1']);
  expect(prefs.learning_concepts).toEqual(['Still learning', 'Unrelated learning']);

  // Explicit save
  await article.locator('[data-save-feedback]').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');

  // After Save, assert selected concept in only requested list and absent from opposite list,
  // and unrelated entries are preserved
  await expect.poll(async () => {
    settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
    const p = settings.config.profiles[profileName].preferences;
    return { known: p.known_concepts, learning: p.learning_concepts };
  }).toEqual({
    known: ['Already known', 'Unrelated known'],
    learning: ['Still learning', 'Unrelated learning', 'concept 1'],
  });

  // Assert feedback and saving caused no additional model calls
  calls = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  expect(calls).toBe(callsAfterGenerate);

  // Reset cancels the temporary feedback action as well as its preferences.
  await article.locator('[data-feedback="known"]').first().click();
  await page.locator('#learning-settings > summary').click();
  await page.locator('#learning-reset').click();
  await article.locator('.feedback-term').fill('Concept 1');
  const savesAfterReset = [];
  page.on('request', request => {
    if (request.method() === 'PUT') savesAfterReset.push(request.url());
  });
  await article.locator('[data-save-feedback]').click();
  await expect(page.locator('#learning-status')).toContainText('Choose a concept');
  expect(savesAfterReset).toEqual([]);
  settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  expect(settings.config.profiles[profileName].preferences.learning_concepts)
    .toEqual(['Still learning', 'Unrelated learning', 'concept 1']);
});

for (const [scenario, conflictError] of [
  ['stale revision', 'learning preferences changed; reload before saving'],
  ['busy lock', 'learning preferences are being edited; retry after the other save completes'],
]) {
  test(`conflict-draft browser feedback draft survives failed settings save (${scenario}) and saves on retry`, async ({ page }) => {
    const mock = await startMock('valid');
    const repo = await createRepository();
    const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
    await page.goto(app.url);
    await expect(page.locator('#learning-profile')).toBeEnabled();

    const profileName = 'Conflict Draft';
    const seeded = await page.evaluate(async name => {
      const current = await (await fetch('/api/learning')).json();
      current.config.profile = name;
      current.config.profiles[name] = {
        preferences: {
          known_concepts: ['Already known', 'Unrelated known'],
          learning_concepts: ['Still learning', 'Unrelated learning', 'Rust lock guards'],
        },
        languages: {},
      };
      return (await fetch('/api/learning', {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          'x-git-explain-learning-token': current.token,
        },
        body: JSON.stringify({ revision: current.revision, config: current.config }),
      })).json();
    }, profileName);
    expect(seeded.ok).toBe(true);

    await page.reload();
    await expect(page.locator('#learning-profile')).toHaveValue(profileName);

    const article = page.locator('article[data-unit-id]').first();
    await article.getByRole('tab', { name: 'Concepts', exact: true }).click();
    await generate(article);

    const callsAfterGenerate = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
    expect(callsAfterGenerate).toBe(1);

    // Click known feedback and edit the label
    await article.locator('[data-feedback="known"]').first().click();
    await article.locator('.view-learning > summary').click();
    await article.locator('.feedback-term').fill('Rust lock guards');

    // Intercept ONLY the first PUT /api/learning with a 409 JSON error; let subsequent PUTs hit the real server
    let intercepted = false;
    await page.route('**/api/learning', async route => {
      if (route.request().method() === 'PUT' && !intercepted) {
        intercepted = true;
        await route.fulfill({
          status: 409,
          contentType: 'application/json',
          body: JSON.stringify({ ok: false, error: conflictError }),
        });
        return;
      }
      await route.continue();
    });

    // First Save attempt triggers 409 conflict
    await article.locator('[data-save-feedback]').click();
    await expect(page.locator('#learning-status')).toHaveText(conflictError);
    await expect(article.locator('.feedback-term')).toHaveValue('Rust lock guards');

    // Assert saved profile is unchanged after rejection
    let settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
    let prefs = settings.config.profiles[profileName].preferences;
    expect(prefs.known_concepts).toEqual(['Already known', 'Unrelated known']);
    expect(prefs.learning_concepts).toEqual(['Still learning', 'Unrelated learning', 'Rust lock guards']);

    // Retry Save succeeds WITHOUT choosing feedback again
    await article.locator('[data-save-feedback]').click();
    await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');

    // Verify final persisted selected/opposite lists preserve unrelated entries
    await expect.poll(async () => {
      settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
      const p = settings.config.profiles[profileName].preferences;
      return { known: p.known_concepts, learning: p.learning_concepts };
    }).toEqual({
      known: ['Already known', 'Unrelated known', 'Rust lock guards'],
      learning: ['Still learning', 'Unrelated learning'],
    });

    // Verify no extra model calls occurred
    const calls = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
    expect(calls).toBe(callsAfterGenerate);
  });
}

test('profile-clone preserves language overrides and explicitly inherited scalar drafts', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  const current = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  current.config.defaults = { detail: 'balanced', known_languages: ['python'] };
  current.config.profile = 'Original';
  current.config.profiles.Original = {
    preferences: { focus: 'reasoning', detail: 'detailed' },
    languages: { rust: { focus: 'syntax', language_help: 'explain' }, javascript: { focus: 'execution', detail: 'concise' } },
  };
  const seeded = await page.request.put(`${app.url}/api/learning`, {
    headers: { Origin: app.url, 'x-git-explain-learning-token': current.token },
    data: { revision: current.revision, config: current.config },
  });
  expect(seeded.ok()).toBe(true);
  await page.goto(app.url);
  await expect(page.locator('#learning-profile')).toHaveValue('Original');
  await page.locator('#learning-settings > summary').click();
  await page.getByText('Customize saved preferences', { exact: true }).click();
  await page.locator('#learning-edit-target').selectOption('profile');
  await page.locator('#learning-editor [data-preference="detail"]').selectOption('');
  await page.locator('#learning-name').fill('Root clone');
  await page.locator('#learning-save-as').click();
  await expect(page.locator('#learning-profile')).toHaveValue('Root clone');
  let settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  const rootClone = settings.config.profiles['Root clone'];
  expect(rootClone.preferences.detail ?? null).toBeNull();
  expect(rootClone.preferences.focus).toBe('reasoning');
  expect(rootClone.languages).toEqual(settings.config.profiles.Original.languages);
  expect(settings.config.profiles.Original.preferences.detail).toBe('detailed');

  await page.locator('#learning-language').fill('rust');
  await page.locator('#learning-language').press('Tab');
  await expect(page.locator('#learning-editor [data-preference="focus"]')).toHaveValue('syntax');
  await page.locator('#learning-editor [data-preference="detail"]').selectOption('concise');
  await page.locator('#learning-name').fill('Language clone');
  await page.locator('#learning-save-as').click();
  await expect(page.locator('#learning-profile')).toHaveValue('Language clone');
  settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  const clone = settings.config.profiles['Language clone'];
  expect(clone.preferences.focus).toBe('reasoning');
  expect(clone.preferences.language_help ?? null).toBeNull();
  expect(clone.preferences.detail ?? null).toBeNull();
  expect(clone.languages.rust).toMatchObject({ focus: 'syntax', language_help: 'explain', detail: 'concise' });
  expect(clone.languages.javascript).toEqual(settings.config.profiles.Original.languages.javascript);
  expect(settings.config.profiles['Root clone'].languages.rust.detail ?? null).toBeNull();
  expect((await fetch(`${mock.origin}/calls`).then(r => r.json())).calls).toBe(0);
});

test('inherited-lists: preserved when editing saved preferences and profiles', async ({ page }) => {
  const mock = await startMock('valid');
  const repo = await createRepository();
  const app = await startApp({ ...repo, modelUrl: mock.url, mode: 'file' });
  await page.goto(app.url);
  await expect(page.locator('#learning-profile')).toBeEnabled();

  const profileName = 'Inherited Profile';
  const current = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  current.config.defaults = {
    focus: 'balanced',
    detail: 'balanced',
    known_languages: ['rust', 'python'],
    learning_languages: ['typescript'],
    known_frameworks: ['react'],
    learning_frameworks: ['tokio'],
    known_domains: ['networking'],
    learning_domains: ['distributed-systems'],
    known_concepts: ['ownership', 'borrowing'],
    learning_concepts: ['async-await'],
  };
  current.config.profile = profileName;
  current.config.profiles[profileName] = {
    preferences: {
      focus: 'syntax',
      detail: 'concise',
    },
    languages: {
      rust: {
        detail: 'detailed',
      },
    },
  };
  const seededResponse = await page.request.put(`${app.url}/api/learning`, {
    headers: {
      'Content-Type': 'application/json',
      'x-git-explain-learning-token': current.token,
      'Origin': app.url,
    },
    data: { revision: current.revision, config: current.config },
  });
  const seeded = await seededResponse.json();
  expect(seeded.ok).toBe(true);

  await page.reload();
  await expect(page.locator('#learning-profile')).toHaveValue(profileName);

  // Initial effective policy check: inherits defaults
  let resolved = await page.request.post(`${app.url}/api/learning/resolve`, {
    data: { language: 'rust', selection: { profile: profileName } },
  }).then(r => r.json());
  expect(resolved.ok).toBe(true);
  expect(resolved.policy.known_languages).toEqual(['python', 'rust']);
  expect(resolved.policy.learning_languages).toEqual(['typescript']);

  // Open saved preferences editor for the selected profile
  await page.locator('#learning-settings > summary').click();
  await page.getByText('Customize saved preferences', { exact: true }).click();
  await page.locator('#learning-edit-target').selectOption('profile');

  // Change unrelated preference (focus: 'code' -> 'execution')
  await page.locator('#learning-editor [data-preference="focus"]').selectOption('execution');

  // Save preferences
  await page.locator('#learning-save').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');

  // Verify unset lists remain unset/null and effective inherited knowledge is preserved
  let settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  let profilePrefs = settings.config.profiles[profileName].preferences;
  expect(profilePrefs.known_languages ?? null).toBeNull();
  expect(profilePrefs.learning_languages ?? null).toBeNull();
  expect(profilePrefs.known_frameworks ?? null).toBeNull();
  expect(profilePrefs.learning_frameworks ?? null).toBeNull();
  expect(profilePrefs.known_domains ?? null).toBeNull();
  expect(profilePrefs.learning_domains ?? null).toBeNull();
  expect(profilePrefs.known_concepts ?? null).toBeNull();
  expect(profilePrefs.learning_concepts ?? null).toBeNull();

  resolved = await page.request.post(`${app.url}/api/learning/resolve`, {
    data: { language: 'rust', selection: { profile: profileName } },
  }).then(r => r.json());
  expect(resolved.policy.known_languages).toEqual(['python', 'rust']);
  expect(resolved.policy.learning_languages).toEqual(['typescript']);

  // Also test language-specific override with no lists:
  // Select language 'rust', change unrelated detail, and save
  await page.locator('#learning-language').fill('rust');
  await page.locator('#learning-language').dispatchEvent('change');
  await page.locator('#learning-editor [data-preference="detail"]').selectOption('concise');
  await page.locator('#learning-save').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');

  settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  let rustLang = settings.config.profiles[profileName].languages.rust;
  expect(rustLang.known_languages ?? null).toBeNull();
  expect(rustLang.learning_languages ?? null).toBeNull();

  resolved = await page.request.post(`${app.url}/api/learning/resolve`, {
    data: { language: 'rust', selection: { profile: profileName } },
  }).then(r => r.json());
  expect(resolved.policy.known_languages).toEqual(['python', 'rust']);

  // Clear language override
  await page.locator('#learning-language').fill('');
  await page.locator('#learning-language').dispatchEvent('change');

  // Verify Save as preserves inheritance
  const clonedProfileName = 'Cloned Profile';
  await page.locator('#learning-name').fill(clonedProfileName);
  await page.locator('#learning-save-as').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');

  settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  let clonedPrefs = settings.config.profiles[clonedProfileName].preferences;
  expect(clonedPrefs.known_languages ?? null).toBeNull();
  expect(clonedPrefs.learning_languages ?? null).toBeNull();

  resolved = await page.request.post(`${app.url}/api/learning/resolve`, {
    data: { language: 'rust', selection: { profile: clonedProfileName } },
  }).then(r => r.json());
  expect(resolved.policy.known_languages).toEqual(['python', 'rust']);
  expect(resolved.policy.learning_languages).toEqual(['typescript']);

  // Explicit empty overrides a nonempty default:
  // Uncheck inherit checkbox for known_languages while input is empty
  const inheritKnown = page.getByRole('checkbox', { name: 'Inherit known languages', exact: true });
  await expect(inheritKnown).toBeChecked();
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await inheritKnown.focus();
  await page.keyboard.press('Space');
  await expect(inheritKnown).not.toBeChecked();
  await expect(page.locator('[data-list="known_languages"]')).toHaveValue('');
  await page.locator('#learning-save').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');

  settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  clonedPrefs = settings.config.profiles[clonedProfileName].preferences;
  expect(clonedPrefs.known_languages).toEqual([]);

  resolved = await page.request.post(`${app.url}/api/learning/resolve`, {
    data: { language: 'rust', selection: { profile: clonedProfileName } },
  }).then(r => r.json());
  expect(resolved.policy.known_languages).toEqual([]);

  // Checking inherit restores it
  await inheritKnown.check();
  await page.locator('#learning-save').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');

  settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  clonedPrefs = settings.config.profiles[clonedProfileName].preferences;
  expect(clonedPrefs.known_languages ?? null).toBeNull();

  resolved = await page.request.post(`${app.url}/api/learning/resolve`, {
    data: { language: 'rust', selection: { profile: clonedProfileName } },
  }).then(r => r.json());
  expect(resolved.policy.known_languages).toEqual(['python', 'rust']);

  // Typing into data-list automatically unchecks inherit checkbox
  const inheritLearning = page.locator('[data-list-inherit="learning_languages"]');
  await expect(inheritLearning).toBeChecked();
  await page.locator('[data-list="learning_languages"]').fill('c++');
  await expect(inheritLearning).not.toBeChecked();
  await page.locator('#learning-save').click();
  await expect(page.locator('#learning-status')).toHaveText('Saved learning preferences.');

  settings = await page.request.get(`${app.url}/api/learning`).then(r => r.json());
  clonedPrefs = settings.config.profiles[clonedProfileName].preferences;
  expect(clonedPrefs.learning_languages).toEqual(['c++']);

  // Verify no model generation occurred for any editor action
  const calls = (await fetch(`${mock.origin}/calls`).then(r => r.json())).calls;
  expect(calls).toBe(0);
});
