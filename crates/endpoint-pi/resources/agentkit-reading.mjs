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
    identities(readEntries) {
      // Reopened/pre-completion sessions have no process-local relation to
      // resolve. Do not enumerate public history until one can exist.
      if (ended.size === 0) return [];
      const byMessage = new Map();
      for (const entry of readEntries()) {
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
