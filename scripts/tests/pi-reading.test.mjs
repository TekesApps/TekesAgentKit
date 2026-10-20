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
