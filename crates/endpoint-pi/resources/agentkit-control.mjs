import { Type } from '@sinclair/typebox';
// Host control over Pi's public extension API; never adds private RPC commands or parses logs.
export default function (pi) {
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
