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

export { startMock, createRepository, startApp, git, root };
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
