import { randomUUID } from 'node:crypto';
// Process-local identities only. Neither message text nor timestamps establish
// a relation. A receipt requires the exact finalized object exposed by Pi's
// public message_end event to be stored in a public SessionMessageEntry.
export function createAssistantReading() {
  let active;
  const ended = new Map();
  return {
    start() { const id = randomUUID(); active = id; return id; },
    end(message) {
      if (!active || message?.role !== 'assistant') return;
      ended.set(active, message); active = undefined;
      while (ended.size > 256) ended.delete(ended.keys().next().value);
    },
    resetActive() { active = undefined; },
    current() { return active; },
    snapshot(sessionManager) {
      const entries = sessionManager.getEntries();
      return { entries, leafId: sessionManager.getLeafId(), readingIdentities: this.identities(entries) };
    },
    identities(entries) {
      const byMessage = new Map();
      for (const entry of entries) {
        if (entry.type !== 'message' || entry.message?.role !== 'assistant' || typeof entry.id !== 'string' || !entry.id) continue;
        byMessage.set(entry.message, byMessage.has(entry.message) && byMessage.get(entry.message) !== entry.id ? null : entry.id);
      }
      const occurrences = new Map();
      for (const message of ended.values()) occurrences.set(message, (occurrences.get(message) ?? 0) + 1);
      return [...ended].flatMap(([streamId, message]) => {
        const entryId = byMessage.get(message);
        return entryId && occurrences.get(message) === 1 ? [{ streamId, entryId }] : [];
      });
    },
  };
}

// Pi 1.3 rpc-mode notify emits this exact envelope with crypto.randomUUID().
// JSON.stringify(message) can at most double the bytes of this already-JSON
// string (only quote/backslash need another escape); UTF-8 stays UTF-8.
const notifyEnvelopeBytes = Buffer.byteLength(JSON.stringify({
  type: 'extension_ui_request', id: '00000000-0000-4000-8000-000000000000',
  method: 'notify', message: '', notifyType: 'info',
}) + '\n');
export function readingSnapshotControlMessage(id, snapshot, maxRecordBytes = 64 * 1024 * 1024) {
  const prefix = 'agentkit-control/v1:';
  const message = prefix + JSON.stringify({ id, success: true, data: snapshot });
  if (notifyEnvelopeBytes + 2 * Buffer.byteLength(message) <= maxRecordBytes) return message;
  // A compact, explicit signal preserves get_entries' original non-nested wire
  // range. The consumer must take a fresh standard snapshot and fresh identities.
  return prefix + JSON.stringify({ id, success: true, data: { transfer: 'standard_entries' } });
}
