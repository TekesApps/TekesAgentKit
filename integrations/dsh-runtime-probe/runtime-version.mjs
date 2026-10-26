import { createRequire } from 'node:module';
import { isAbsolute } from 'node:path';
const VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
export function resolveDSHRuntimeVersion(runtimeEntry) {
  if (typeof runtimeEntry !== 'string' || !isAbsolute(runtimeEntry)) throw new Error('DSH runtime entry is unavailable');
  const runtimeRequire = createRequire(runtimeEntry);
  const officialPackage = runtimeRequire.resolve('@deepseek-ai/dsh-app-boot');
  const { getDshRuntimeVersion } = runtimeRequire(officialPackage);
  if (typeof getDshRuntimeVersion !== 'function') throw new Error('Official DSH version function is unavailable');
  const version = getDshRuntimeVersion();
  if (typeof version !== 'string' || !VERSION.test(version)) throw new Error('Official DSH version value is invalid');
  return { product: '@deepseek-ai/dsh', source: '@deepseek-ai/dsh-app-boot#getDshRuntimeVersion', version };
}
