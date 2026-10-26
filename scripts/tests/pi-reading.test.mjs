import test from 'node:test';
import assert from 'node:assert/strict';
import { createAssistantReading } from '../../crates/endpoint-pi/resources/agentkit-reading.mjs';
const assistant = () => ({ role: 'assistant', timestamp: 100, content: [{ type: 'text', text: 'same payload' }] });
const entry = (id, message) => ({ id, type: 'message', message });
test('a receipt uses only the stored finalized object, not equal content or timestamp', () => {
  const reading = createAssistantReading(), final = assistant(), older = assistant();
  const id = reading.start(); reading.end(final);
  assert.deepEqual(reading.identities([entry('older', older)]), []);
  assert.deepEqual(reading.identities([entry('older', older), entry('actual', final)]), [{ streamId: id, entryId: 'actual' }]);
  final.timestamp = 200; final.content[0].text = 'extension finalized content';
  assert.equal(reading.identities([entry('actual', final)])[0].entryId, 'actual');
});
test('an end before persistence remains unassociated until a public native entry exists', () => {
  const reading = createAssistantReading(), final = assistant(); const id = reading.start(); reading.end(final);
  assert.deepEqual(reading.identities([]), []);
  assert.deepEqual(reading.identities([entry('committed', final)]), [{ streamId: id, entryId: 'committed' }]);
});
test('reset or missing starts never adopt another lifecycle', () => {
  const reading = createAssistantReading(), final = assistant(); reading.end(final);
  assert.deepEqual(reading.identities([entry('x', final)]), []);
  reading.start(); reading.resetActive(); reading.end(final);
  assert.deepEqual(reading.identities([entry('x', final)]), []);
  const id = reading.start(); reading.end(final); reading.end(assistant());
  assert.deepEqual(reading.identities([entry('x', final)]), [{ streamId: id, entryId: 'x' }]);
});
test('ambiguous stored objects or repeated lifecycles fail closed', () => {
  const reading = createAssistantReading(), final = assistant(); reading.start(); reading.end(final);
  assert.deepEqual(reading.identities([entry('one', final), entry('two', final)]), []);
  reading.start(); reading.end(final);
  assert.deepEqual(reading.identities([entry('one', final)]), []);
});
test('the process-local receipt cache is bounded without persisting or rewriting entries', () => {
  const reading = createAssistantReading(), entries = [], ids = [];
  for (let i = 0; i < 257; i++) { const message = assistant(); ids.push(reading.start()); reading.end(message); entries.push(entry(String(i), message)); }
  const receipts = reading.identities(entries);
  assert.equal(receipts.length, 256); assert(!receipts.some(receipt => receipt.streamId === ids[0]));
  assert.equal(entries.length, 257); assert.equal(entries[0].id, '0'); assert.equal(entries[0].message.timestamp, 100);
});

test('snapshot reads entries once and binds identities to that exact public result', () => {
  const reading = createAssistantReading(), final = assistant(), older = assistant();
  const streamId = reading.start(); reading.end(final);
  const before = [entry('older', older)], committed = [entry('actual', final)];
  let calls = 0;
  const manager = {
    getEntries() { calls++; return calls === 1 ? before : committed; },
    getLeafId() { return calls === 1 ? 'older' : 'actual'; },
  };
  const first = reading.snapshot(manager);
  assert.equal(calls, 1);
  assert.equal(first.entries, before);
  assert.equal(first.leafId, 'older');
  assert.deepEqual(first.readingIdentities, []);
  const second = reading.snapshot(manager);
  assert.equal(calls, 2);
  assert.equal(second.entries, committed);
  assert.equal(second.leafId, 'actual');
  assert.deepEqual(second.readingIdentities, [{ streamId, entryId: 'actual' }]);
});
test('snapshot preserves ambiguous-object rejection and never rewrites public entries', () => {
  const reading = createAssistantReading(), final = assistant();
  reading.start(); reading.end(final);
  const entries = Object.freeze([Object.freeze(entry('one', final)), Object.freeze(entry('two', final))]);
  const before = JSON.stringify(entries);
  const snapshot = reading.snapshot(Object.freeze({
    getEntries() { return entries; }, getLeafId() { return 'two'; },
  }));
  assert.equal(snapshot.entries, entries);
  assert.equal(snapshot.leafId, 'two');
  assert.deepEqual(snapshot.readingIdentities, []);
  assert.equal(JSON.stringify(entries), before);
});

const rpcNotifyLine = message => JSON.stringify({
  type: 'extension_ui_request', id: '00000000-0000-4000-8000-000000000000',
  method: 'notify', message, notifyType: 'info',
}) + '\n';
const controlData = message => JSON.parse(message.slice('agentkit-control/v1:'.length)).data;
test('combined snapshot notification respects the actual nested UTF-8 JSONL envelope', async () => {
  const { readingSnapshotControlMessage } = await import('../../crates/endpoint-pi/resources/agentkit-reading.mjs');
  for (const text of ['汉字🙂', '\u2028\u2029', '"\\\n\u0000', '\ud800']) {
    const snapshot = { entries: [entry('one', { ...assistant(), content: [{ type: 'text', text: text.repeat(20) }] })], leafId: 'one', readingIdentities: [] };
    const message = readingSnapshotControlMessage('request', snapshot, 4096);
    assert.deepEqual(controlData(message), snapshot);
    assert(Buffer.byteLength(rpcNotifyLine(message)) <= 4096);
  }
});
test('large previously valid get_entries history uses a compact standard-channel signal', async () => {
  const { readingSnapshotControlMessage } = await import('../../crates/endpoint-pi/resources/agentkit-reading.mjs');
  const entries = Array.from({ length: 200 }, (_, i) => entry(String(i), { ...assistant(), content: [{ type: 'text', text: '"'.repeat(100000) }] }));
  const snapshot = { entries, leafId: '199', readingIdentities: [] }, limit = 64 * 1024 * 1024;
  const original = JSON.stringify({ type: 'response', id: 'request', command: 'get_entries', success: true, data: { entries, leafId: '199' } }) + '\n';
  const unsafe = 'agentkit-control/v1:' + JSON.stringify({ id: 'request', success: true, data: snapshot });
  assert(Buffer.byteLength(original) < limit);
  assert(Buffer.byteLength(rpcNotifyLine(unsafe)) > limit);
  const message = readingSnapshotControlMessage('request', snapshot);
  assert.deepEqual(controlData(message), { transfer: 'standard_entries' });
  assert(Buffer.byteLength(rpcNotifyLine(message)) < limit);
  assert.equal(snapshot.entries, entries);
});
test('bounded notifications fit the unchanged line limit at small pressure boundaries', async () => {
  const { readingSnapshotControlMessage } = await import('../../crates/endpoint-pi/resources/agentkit-reading.mjs');
  for (const limit of [512, 1024, 2048, 4096]) {
    for (const text of ['x', '"', '\\', '汉🙂']) {
      const snapshot = { entries: [entry('one', { ...assistant(), content: [{ type: 'text', text: text.repeat(500) }] })], leafId: 'one', readingIdentities: [] };
      const message = readingSnapshotControlMessage('request', snapshot, limit);
      const data = controlData(message);
      assert(Buffer.byteLength(rpcNotifyLine(message)) <= limit);
      assert.deepEqual(data, data.transfer === 'standard_entries' ? { transfer: 'standard_entries' } : snapshot);
    }
  }
});
