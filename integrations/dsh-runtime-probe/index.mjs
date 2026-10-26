import { resolveDSHRuntimeVersion } from './runtime-version.mjs';
export const name = 'tekes-agentkit-dsh-runtime-probe';
export function registerRuntimeVersionHTTP(ctx, { runtimeEntry = process.argv[1] } = {}) {
  return ctx.webServer.register({ kind: 'exact', path: '/api/agentkit/harness-version', handler(req, res) {
    const admission = ctx.connection.admit(req);
    if ('rejection' in admission) { res.writeHead(admission.rejection); res.end(); return; }
    if (req.method !== 'GET') { res.writeHead(405); res.end(); return; }
    try {
      const evidence = resolveDSHRuntimeVersion(runtimeEntry);
      res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
      res.end(JSON.stringify(evidence));
    } catch {
      res.writeHead(503, { 'content-type': 'application/json', 'cache-control': 'no-store' });
      res.end(JSON.stringify({ error: 'DSH official runtime version unavailable' }));
    }
  } });
}
export default function apply(ctx) {
  ctx.inject(['connection', 'webServer'], host => {
    host.effect(() => registerRuntimeVersionHTTP(host));
  });
}
