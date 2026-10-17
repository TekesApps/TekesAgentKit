import { Type } from '@sinclair/typebox';
import { createHash, randomUUID } from 'node:crypto';
// Host control over Pi's public extension API; never adds private RPC commands or parses logs.
export default function (pi) {
  const digest = text => createHash('sha256').update(text).digest('hex');
  const prepared = [], admitted = [];
  let runID;
  pi.on('input', async event => {
    if (event.source !== 'rpc') return;
    const index = prepared.findIndex(item => item.hash === digest(event.text));
    if (index >= 0) admitted.push({ ...prepared.splice(index, 1)[0], mode: event.streamingBehavior ?? 'started' });
  });
  pi.on('message_end', async event => {
    if (event.message.role !== 'user') return;
    const content = event.message.content;
    const text = typeof content === 'string' ? content : content.filter(block => block.type === 'text').map(block => block.text).join('\n');
    const hash = digest(text);
    let index = admitted.findIndex(item => item.hash === hash);
    if (index < 0) index = admitted.findIndex(item => item.hasImages && text.startsWith(item.text + '\n\n'));
    if (index >= 0) {
      const item = admitted.splice(index, 1)[0];
      pi.appendEntry('agentkit.inputReceipt.v1', { timestamp: event.message.timestamp, hash, rpcId: item.id, mode: item.mode });
    }
  });
  pi.on('agent_start', async () => {
    runID = randomUUID();
    pi.appendEntry('agentkit.run.v1', { id: runID, phase: 'started', timestamp: Date.now() });
  });
  pi.on('agent_settled', async event => {
    if (runID) pi.appendEntry('agentkit.run.v1', { id: runID, phase: 'settled', aborted: event.aborted, timestamp: Date.now() });
    runID = undefined;
  });
  pi.registerTool({
    name: 'agentkit_goal', label: 'Goal state',
    description: 'Record completion or a concrete blocker for the active user goal.',
    parameters: Type.Object({ phase: Type.Union([Type.Literal('complete'), Type.Literal('blocked')]), reason: Type.Optional(Type.String()) }),
    execute: async (_id, args, _signal, _update, ctx) => {
      const entry = [...ctx.sessionManager.getBranch()].reverse().find(entry => entry.type === 'custom' && entry.customType === 'agentkit.goal.v1');
      const record = entry?.data;
      if (!record?.value || record.value.phase !== 'active' || !['complete', 'blocked'].includes(args.phase)) throw new Error('No active goal');
      const revision = record.revision + 1;
      const goal = { ...record.value, revision, phase: args.phase, activation: 'disarmed', updatedAt: Date.now(), blockedReason: args.phase === 'blocked' ? { message: args.reason ?? 'Model reported a blocker' } : null };
      pi.appendEntry('agentkit.goal.v1', { revision, value: goal });
      return { content: [{ type: 'text', text: JSON.stringify(goal) }], details: goal };
    },
  });
  pi.registerCommand('agentkit-control', {
    description: 'AgentKit authenticated host control',
    handler: async (encoded, ctx) => {
      const request = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8'));
      if (!process.env.AGENTKIT_CONTROL_TOKEN || request.token !== process.env.AGENTKIT_CONTROL_TOKEN || typeof request.id !== 'string') throw new Error('Invalid host control');
      try {
        let data;
        switch (request.method) {
          case 'fork.at': {
            let replied = false;
            data = await ctx.fork(request.params.entryId, {
              position: 'at',
              withSession: async fresh => {
                fresh.ui.notify('agentkit-control/v1:' + JSON.stringify({ id: request.id, success: true, data: { cancelled: false } }), 'info');
                replied = true;
              },
            });
            if (replied) return;
            break;
          }
          case 'input.cancel': {
            for (const queue of [prepared, admitted]) { const index = queue.findIndex(item => item.id === request.params.id); if (index >= 0) queue.splice(index, 1); }
            data = null; break;
          }
          case 'input.prepare': {
            if (prepared.length + admitted.length >= 256) throw new Error('Input receipt limit');
            const { id, text, hasImages } = request.params;
            if (typeof id !== 'string' || !id || typeof text !== 'string') throw new Error('Invalid input receipt');
            prepared.push({ id, text, hash: digest(text), hasImages: hasImages === true }); data = null; break;
          }
          case 'settings.get': data = pi.getSettings(); break;
          case 'tools.list': data = pi.getAllTools(); break;
          case 'tools.select': pi.setActiveTools(request.params.tools); data = pi.getActiveTools(); break;
          case 'skills.list': data = pi.getCommands().filter(command => command.source === 'skill').map(command => ({ name: command.name.replace(/^skill:/, ''), description: command.description ?? '', path: command.sourceInfo?.path, modelInvocable: true })); break;
          case 'entry.append': pi.appendEntry(request.params.type, request.params.data); data = null; break;
          default: throw new Error('Unknown host control');
        }
        ctx.ui.notify('agentkit-control/v1:' + JSON.stringify({ id: request.id, success: true, data }), 'info');
      } catch (error) {
        ctx.ui.notify('agentkit-control/v1:' + JSON.stringify({ id: request.id, success: false, error: String(error) }), 'info');
      }
    },
  });
}
