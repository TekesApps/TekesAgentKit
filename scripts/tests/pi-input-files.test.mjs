import test from 'node:test';
import assert from 'node:assert/strict';
import { inputReceiptPayload, snapshotInputFiles } from '../../crates/endpoint-pi/resources/agentkit-input-files.mjs';

const id = 'aBcDeF01-2345-6789-abcd-ef0123456789';
const entry = (overrides = {}) => ({
  contentIndex: 1,
  precedingImageCount: 0,
  file: { attachmentId: id, name: 'notes.txt', bytes: 12 },
  ...overrides,
});

test('legacy prepare without files snapshots as empty and legacy receipt omits files', () => {
  const files = snapshotInputFiles(undefined);
  assert.deepEqual(files, []);
  assert.deepEqual(inputReceiptPayload({ rpcId: 'old', hash: 'abc' }, files), { rpcId: 'old', hash: 'abc' });
});

test('file metadata is copied and frozen at prepare time', () => {
  const source = [entry()];
  const files = snapshotInputFiles(source);
  source[0].file.name = 'later.txt';
  source[0].file.bytes = 100;
  source[0].contentIndex = 4;
  assert.deepEqual(files, [{ contentIndex: 1, precedingImageCount: 0, file: { attachmentId: id, name: 'notes.txt', bytes: 12 } }]);
  assert(Object.isFrozen(files) && Object.isFrozen(files[0]) && Object.isFrozen(files[0].file));
});

test('actual receipt payload assembly preserves the frozen file metadata', () => {
  const files = snapshotInputFiles([entry({ contentIndex: 2, precedingImageCount: 1 })]);
  const payload = inputReceiptPayload({ timestamp: 50, hash: 'hash', rpcId: 'rpc-1', mode: 'started' }, files);
  assert.deepEqual(payload, {
    timestamp: 50,
    hash: 'hash',
    rpcId: 'rpc-1',
    mode: 'started',
    files: [{ contentIndex: 2, precedingImageCount: 1, file: { attachmentId: id, name: 'notes.txt', bytes: 12 } }],
  });
});

test('UUIDs accept standard upper and lower case while preserving the supplied ID', () => {
  const upper = 'ABCDEF01-2345-6789-ABCD-EF0123456789';
  assert.equal(snapshotInputFiles([entry({ file: { attachmentId: upper, name: 'a', bytes: 1 } })])[0].file.attachmentId, upper);
});

test('older file metadata may omit precedingImageCount and defaults it to zero', () => {
  const { precedingImageCount } = snapshotInputFiles([{
    contentIndex: 0,
    file: { attachmentId: id, name: 'legacy.txt', bytes: 1 },
  }])[0];
  assert.equal(precedingImageCount, 0);
});

test('the full set is rejected atomically when a later item is invalid', () => {
  assert.throws(() => snapshotInputFiles([
    entry(),
    entry({ contentIndex: 2, file: { attachmentId: 'bad', name: 'later.txt', bytes: 1 } }),
  ]));
});

test('rejects invalid arrays, ordering, duplicate positions, and image counts', () => {
  assert.throws(() => snapshotInputFiles({}));
  assert.throws(() => snapshotInputFiles(Array.from({ length: 9 }, (_, i) => entry({ contentIndex: i }))));
  assert.throws(() => snapshotInputFiles([entry(), entry({ contentIndex: 1 })]));
  assert.throws(() => snapshotInputFiles([entry({ contentIndex: 2, precedingImageCount: 2 }), entry({ contentIndex: 3, precedingImageCount: 1 })]));
  assert.throws(() => snapshotInputFiles([entry({ contentIndex: 0, precedingImageCount: 1 })]));
  assert.throws(() => snapshotInputFiles([entry({ contentIndex: Number.MAX_SAFE_INTEGER + 1 })]));
});

test('rejects malformed metadata and unsafe names', () => {
  for (const file of [
    { attachmentId: 'not-a-uuid', name: 'a', bytes: 1 },
    { attachmentId: id, name: '', bytes: 1 },
    { attachmentId: id, name: '.', bytes: 1 },
    { attachmentId: id, name: '..', bytes: 1 },
    { attachmentId: id, name: 'dir/a', bytes: 1 },
    { attachmentId: id, name: 'dir\\a', bytes: 1 },
    { attachmentId: id, name: 'bad\u0000name', bytes: 1 },
    { attachmentId: id, name: 'a', bytes: 0 },
    { attachmentId: id, name: 'a', bytes: 1.5 },
    { attachmentId: id, name: 'a', bytes: 8 * 1024 * 1024 + 1 },
  ]) assert.throws(() => snapshotInputFiles([entry({ file })]));
});

test('enforces the aggregate 16 MiB file budget', () => {
  assert.doesNotThrow(() => snapshotInputFiles([
    entry({ contentIndex: 1, file: { attachmentId: id, name: 'a', bytes: 8 * 1024 * 1024 } }),
    entry({ contentIndex: 2, file: { attachmentId: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', name: 'b', bytes: 8 * 1024 * 1024 } }),
  ]));
  assert.throws(() => snapshotInputFiles([
    entry({ contentIndex: 1, file: { attachmentId: id, name: 'a', bytes: 8 * 1024 * 1024 } }),
    entry({ contentIndex: 2, file: { attachmentId: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', name: 'b', bytes: 8 * 1024 * 1024 } }),
    entry({ contentIndex: 3, file: { attachmentId: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb', name: 'c', bytes: 1 } }),
  ]));
});
