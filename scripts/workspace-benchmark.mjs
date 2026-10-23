// HTTP-stage benchmark. Browser paint/heap and NVDA measurements are separate.
// Usage: node scripts/workspace-benchmark.mjs --binary ABSOLUTE_EXE [--counts 10,100,1000,5000]
import { spawn, spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { performance } from 'node:perf_hooks';

const args = process.argv.slice(2);
const option = (name, fallback) => args.includes(name) ? args[args.indexOf(name) + 1] : fallback;
const binary = option('--binary');
if (!binary || !path.isAbsolute(binary)) throw new Error('Provide --binary with the frozen executable absolute path');
const counts = option('--counts', '10,100,1000,5000').split(',').map(Number);
if (counts.some(count => !Number.isInteger(count) || count < 1 || count > 10000)) throw new Error('Invalid section counts');
const modes = option('--modes', 'file,diff,daemon').split(',');
if (modes.some(mode => !['file', 'diff', 'daemon'].includes(mode))) throw new Error('Invalid modes');
const sectionLines = Number(option('--section-lines', '0'));
if (!Number.isInteger(sectionLines) || sectionLines < 0 || sectionLines > 10000) throw new Error('Invalid section line count');
const output = path.resolve(option('--output', 'target/learning-evaluation/agy-tasks/ui-redesign/http-benchmark.json'));
const git = (cwd, ...values) => {
  const result = spawnSync('git', values, { cwd, encoding: 'utf8' });
  if (result.status !== 0) throw new Error(result.stderr);
};
const freePort = () => new Promise(resolve => {
  const listener = net.createServer();
  listener.listen(0, '127.0.0.1', () => {
    const port = listener.address().port;
    listener.close(() => resolve(port));
  });
});
async function timed(url) {
  const start = performance.now();
  const response = await fetch(url);
  const text = await response.text();
  return { status: response.status, bytes: Buffer.byteLength(text), milliseconds: performance.now() - start, text };
}
const results = [];
const browser = args.includes('--browser') ? await (await import('@playwright/test')).chromium.launch() : null;
try { for (const mode of modes) for (const count of counts) {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'git-explain-workspace-benchmark-'));
  let child;
  try {
    git(directory, 'init', '-q');
    git(directory, 'config', 'user.name', 'Workspace Benchmark');
    git(directory, 'config', 'user.email', 'benchmark@example.test');
    await mkdir(path.join(directory, 'src'));
    // Approx. 1 KiB per section, with a long line for source reflow checks.
    let source = Array.from({ length: count }, (_, index) => `fn section_${index}() -> usize {\n    // ${'source '.repeat(135)}\n    ${index}\n}\n`).join('\n');
    if (sectionLines) source = source.replace('    0\n}', `    // ${'long-line '.repeat(2048)}\n${'    // long section reading fixture\n'.repeat(sectionLines)}    0\n}`);
    const selected = path.join(directory, 'src/lib.rs');
    await writeFile(selected, source);
    git(directory, 'add', 'src');
    git(directory, 'commit', '-qm', 'benchmark base');
    if (mode !== 'file') await writeFile(selected, source.replace('    0\n}', '    42\n}'));
    const port = await freePort();
    const config = path.join(directory, 'user.toml');
    await writeFile(config, `[model]\nprofile='benchmark'\n[profiles.benchmark]\nprovider='openai_compatible'\nbase_url='http://127.0.0.1:1/v1'\nmodel='unused'\n[server]\nport=${port}\nopen_browser=false\n[cache]\nenabled=false\n`);
    const command = mode === 'daemon' ? ['daemon', 'run'] : ['--direct', mode, ...(mode === 'file' ? ['--', 'src'] : [])];
    const start = performance.now();
    child = spawn(binary, command, { cwd: directory, env: { ...process.env, GIT_EXPLAIN_USER_CONFIG: config, GIT_EXPLAIN_DAEMON_DIR: path.join(directory, 'daemon'), GIT_EXPLAIN_DAEMON_PORT: String(port) }, stdio: ['ignore', 'pipe', 'pipe'] });
    await new Promise((resolve, reject) => {
      let stdout = '', stderr = '';
      const timer = setTimeout(() => reject(new Error(`Server startup timed out: ${stderr}`)), 120000);
      child.on('error', reject);
      child.on('exit', code => { clearTimeout(timer); reject(new Error(`Server exited ${code}: ${stderr}`)); });
      child.stderr.on('data', chunk => { stderr += chunk; });
      child.stdout.on('data', chunk => {
        stdout += chunk;
        if (stdout.includes(mode === 'daemon' ? 'git-explain daemon listening on ' : 'git explain: ')) { clearTimeout(timer); resolve(); }
      });
    });
    const readinessMs = performance.now() - start;
    const origin = `http://127.0.0.1:${port}`;
    let base = origin, api = `${origin}/api/ui/v1`, sessionCreationMs = null;
    if (mode === 'daemon') {
      const metadata = JSON.parse(await readFile(path.join(directory, 'daemon/daemon.json'), 'utf8'));
      const sessionStart = performance.now();
      const response = await fetch(`${origin}/api/repos/open`, { method: 'POST', headers: { 'Content-Type': 'application/json', 'x-git-explain-control-token': metadata.control_token }, body: JSON.stringify({ repo_root: directory, profile: 'benchmark' }) });
      const opened = await response.json();
      if (!opened.ok) throw new Error(JSON.stringify(opened));
      sessionCreationMs = performance.now() - sessionStart;
      base = `${origin}/sessions/${opened.session_id}`;
      api = `${origin}/api/sessions/${opened.session_id}/ui/v1`;
    }
    const legacy = await timed(`${base}/legacy`);
    const shell = await timed(`${base}/workspace`);
    const index = await timed(`${api}/index`);
    let sections = 0, detail = null, warmDetail = null;
    if (index.status === 200) {
      const data = JSON.parse(index.text);
      sections = data.files.reduce((total, file) => total + file.sections.length, 0);
      const section = data.files[0]?.sections[0];
      if (section) {
        const url = `${api}/sections/${section.id}?snapshot_id=${data.snapshot_id}`;
        detail = await timed(url);
        warmDetail = await timed(url);
      }
    }
    const metric = value => value && ({ status: value.status, bytes: value.bytes, milliseconds: value.milliseconds });
    let browserMetrics = null;
    if (browser) {
      const context = await browser.newContext();
      const page = await context.newPage();
      const requests = [];
      page.on('request', request => requests.push({ url: request.url(), method: request.method() }));
      const paintStart = performance.now();
      await page.goto(base);
      await page.getByRole('link', { name: /src\/lib.rs/ }).waitFor();
      const usableMs = performance.now() - paintStart;
      const initialSourceRequests = requests.filter(request => request.url.includes('/ui/v1/sections/')).length;
      const initialPostRequests = requests.filter(request => request.method === 'POST').length;
      const listStart = performance.now();
      await page.getByRole('link', { name: /src\/lib.rs/ }).click();
      await page.getByRole('link', { name: new RegExp(`section_${count - 1}.*function`) }).waitFor();
      const listPaintMs = performance.now() - listStart;
      // File selection now starts the first source load; measure the complete file gesture.
      await page.locator('[data-source-view]').waitFor();
      const sourcePaintMs = performance.now() - listStart;
      let uncachedEventToFrameMs = null;
      let cachedSwitchMs = null;
      let cachedEventToFrameMs = null;
      if (count > 1) {
        uncachedEventToFrameMs = await page.evaluate(sectionId => new Promise((resolve, reject) => {
          const reader = document.querySelector('.reader-container');
          const link = document.querySelector(`a.section-link[href*="section=${sectionId}"]`);
          if (!reader || !link) { reject(new Error('Missing uncached navigation target')); return; }
          const start = performance.now();
          let scheduled = false;
          const timer = setTimeout(() => { observer.disconnect(); reject(new Error('Uncached source did not settle')); }, 5000);
          const observer = new MutationObserver(() => {
            if (!scheduled && document.querySelector('[data-source-view]')?.textContent?.startsWith('fn section_1()')) {
              scheduled = true;
              requestAnimationFrame(() => { clearTimeout(timer); observer.disconnect(); resolve(performance.now() - start); });
            }
          });
          observer.observe(reader, { subtree: true, childList: true, characterData: true });
          link.click();
        }), JSON.parse(index.text).files[0].sections[1].id);
        await page.getByRole('heading', { name: /lib.rs.*section_1$/ }).waitFor();
        await page.locator('[data-source-view]').waitFor();
        const warmStart = performance.now();
        await page.getByRole('link', { name: /section_0\s+function/ }).click();
        await page.getByRole('heading', { name: /lib.rs.*section_0$/ }).waitFor();
        await page.locator('[data-source-view]').waitFor();
        cachedSwitchMs = performance.now() - warmStart;
        await page.getByRole('link', { name: /section_1\s+function/ }).click();
        await page.waitForFunction(() => document.querySelector('[data-source-view]')?.textContent?.startsWith('fn section_1()'));
        cachedEventToFrameMs = await page.evaluate(({ sectionId, source }) => new Promise((resolve, reject) => {
          const reader = document.querySelector('.reader-container');
          const link = document.querySelector(`a.section-link[href*="section=${sectionId}"]`);
          if (!reader || !link) { reject(new Error('Missing benchmark navigation target')); return; }
          const start = performance.now();
          const timer = setTimeout(() => { observer.disconnect(); reject(new Error('Source did not settle')); }, 5000);
          let scheduled = false;
          const observer = new MutationObserver(() => {
            if (!scheduled && document.querySelector('[data-source-view]')?.textContent === source) {
              scheduled = true;
              requestAnimationFrame(() => { clearTimeout(timer); observer.disconnect(); resolve(performance.now() - start); });
            }
          });
          observer.observe(reader, { subtree: true, childList: true, characterData: true, attributes: true });
          link.click();
        }), { sectionId: JSON.parse(index.text).files[0].sections[0].id, source: JSON.parse(detail.text).source });
      }
      const domNodes = await page.evaluate(() => document.querySelectorAll('*').length);
      const cdp = await context.newCDPSession(page);
      await cdp.send('Performance.enable');
      const { metrics } = await cdp.send('Performance.getMetrics');
      const jsHeap = metrics.find(value => value.name === 'JSHeapUsedSize')?.value;
      const generationRequests = requests.filter(request => request.method === 'POST' && /\/(?:file-)?units\//.test(request.url)).length;
      if (generationRequests || initialSourceRequests || initialPostRequests) throw new Error('Navigation submitted an unauthorized source or generation startup request');
      if (mode === 'file' && count === 1000) await page.screenshot({ path: output.replace(/\.json$/, '-reader.png'), fullPage: false });
      browserMetrics = { usable_ms: usableMs, section_list_paint_ms: listPaintMs, selected_source_ms: sourcePaintMs, uncached_event_to_frame_ms: uncachedEventToFrameMs, cached_switch_ms: cachedSwitchMs, cached_event_to_frame_ms: cachedEventToFrameMs, initial_source_requests: initialSourceRequests, initial_post_requests: initialPostRequests, generation_requests: generationRequests, dom_nodes: domNodes, js_heap_bytes: jsHeap };
      await context.close();
    }
    const result = { mode, fixture_sections: count, long_section_lines: sectionLines, indexed_sections: sections, readiness_ms: readinessMs, session_creation_ms: sessionCreationMs, legacy_html: metric(legacy), shell: metric(shell), index: metric(index), detail: metric(detail), warm_detail: metric(warmDetail), browser: browserMetrics };
    results.push(result);
    console.log(JSON.stringify(result));
    await mkdir(path.dirname(output), { recursive: true });
    await writeFile(output, JSON.stringify({ platform: process.platform, arch: process.arch, node: process.version, cpus: os.cpus()[0]?.model, binary, measured_at: new Date().toISOString(), scope: browser ? 'HTTP, server readiness, Chromium usable/list/source timings and heap; no model generation; NVDA not measured. Browser timings include Playwright actionability and focus/scroll.' : 'HTTP and server readiness; no model generation submitted; browser paint, heap and NVDA not measured', results }, null, 2));
  } finally {
    if (child && child.exitCode === null) await new Promise(resolve => { child.once('exit', resolve); child.kill(); });
    const resolved = path.resolve(directory);
    if (path.dirname(resolved) !== path.resolve(os.tmpdir()) || !path.basename(resolved).startsWith('git-explain-workspace-benchmark-')) throw new Error('Unexpected fixture cleanup path');
    await rm(resolved, { recursive: true, force: true, maxRetries: 5 });
  }
} } finally { await browser?.close(); }
