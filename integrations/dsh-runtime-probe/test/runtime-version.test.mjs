import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { registerRuntimeVersionHTTP } from '../index.mjs';
async function runtimeFixture(t, value = '0.2.1-rc.2') {
  const root = await mkdtemp(join(tmpdir(), 'agentkit-dsh-probe-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const entry = join(root, 'cli', 'bin.js');
  const pkg = join(root, 'cli', 'node_modules', '@deepseek-ai', 'dsh-app-boot');
  await mkdir(pkg, { recursive: true });
  await writeFile(entry, '// fixture CLI entry\n');
  await writeFile(join(pkg, 'package.json'), JSON.stringify({ name: '@deepseek-ai/dsh-app-boot', main: 'index.cjs' }));
  await writeFile(join(pkg, 'index.cjs'), `exports.getDshRuntimeVersion = () => ${JSON.stringify(value)};\n`);
  return entry;
}
async function withServer(t, runtimeEntry) {
  const routes = new Map();
  const dispose = registerRuntimeVersionHTTP({
    connection: { admit(req) { return req.headers.authorization === 'Bearer valid' ? { peer: { id: 'owner' } } : { rejection: 401 }; } },
    webServer: { register(route) { routes.set(route.path, route.handler); return () => routes.delete(route.path); } },
  }, { runtimeEntry });
  const server = http.createServer((req, res) => {
    const handler = routes.get(req.url);
    if (handler) void handler(req, res); else { res.writeHead(404); res.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => { dispose(); server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); });
  return `http://127.0.0.1:${server.address().port}/api/agentkit/harness-version`;
}
test('reports official CLI runtime version only after authentication', async t => {
  const url = await withServer(t, await runtimeFixture(t));
  assert.equal((await fetch(url)).status, 401);
  const response = await fetch(url, { headers: { authorization: 'Bearer valid' } });
  assert.equal(response.status, 200);
  assert.equal(response.headers.get('cache-control'), 'no-store');
  assert.deepEqual(await response.json(), { product: '@deepseek-ai/dsh', source: '@deepseek-ai/dsh-app-boot#getDshRuntimeVersion', version: '0.2.1-rc.2' });
});
test('rejects non-GET requests after authentication', async t => {
  const url = await withServer(t, await runtimeFixture(t));
  assert.equal((await fetch(url, { method: 'POST', headers: { authorization: 'Bearer valid' } })).status, 405);
});
test('returns unavailable rather than inferring when official runtime cannot resolve', async t => {
  const url = await withServer(t, '/tmp/no-agentkit-dsh-runtime-entry.js');
  const response = await fetch(url, { headers: { authorization: 'Bearer valid' } });
  assert.equal(response.status, 503);
  assert.deepEqual(await response.json(), { error: 'DSH official runtime version unavailable' });
});
test('rejects malformed official version strings', async t => {
  const url = await withServer(t, await runtimeFixture(t, 'not-a-version'));
  assert.equal((await fetch(url, { headers: { authorization: 'Bearer valid' } })).status, 503);
});
