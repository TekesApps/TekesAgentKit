//! Public TypeScript declarations for AgentKit's shared Web presentation.

pub fn web_presentation_typescript() -> &'static str {
    concat!(
        "export interface RunScope {\n  endpointId: string;\n  threadId: string;\n  sessionId: string;\n}\n",
        "export interface ConnectionPresentation {\n  phase: 'connecting' | 'connected' | 'reconnecting' | 'degraded' | 'failed' | 'stopped';\n  canSubmit: boolean;\n  historyIsStale: boolean;\n  message?: string | null;\n}\n",
        "/** Shared AgentKit Run read model; all permission fields are server-derived. */\n",
        "export interface RunPresentation {\n  scope: RunScope;\n  connectionEpoch: number;\n",
        "  phase: 'loading' | 'idle' | 'running' | 'waitingForInput' | 'interrupted' | 'archived' | 'unavailable';\n",
        "  connection: ConnectionPresentation;\n  canSubmit: boolean;\n  canCancel: boolean;\n  canUpdateQueue: boolean;\n",
        "  actionables: unknown[];\n  queue: unknown[];\n  jobs: unknown[];\n  error?: string | null;\n}\n",
        "/** Remove one currently queued input using the observed connection epoch. */\n",
        "export interface AgentQueueRemoveRequest {\n  sessionId: string;\n  itemId: string;\n  connectionEpoch: number;\n}\n",
        "export type AgentQueueRemoveMethod = 'agent.queue.remove';\n"
    )
}
