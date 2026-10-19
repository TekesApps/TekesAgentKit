# Worker startup notice recovery

Rust presentation preserves the original error message and item identity. A
`session/notice` with classification `worker_launch` and severity `error` remains
active until a later durable `turn/start` proves execution resumed in that same
session. Its metadata status then becomes `recovered`, with the public recovery
sequence and timestamp. Replaying historical journals applies the same rule to
old Kernel errors. Other errors, accepted inputs, connection changes and idle
states do not recover a launch failure. A new launch failure remains active until
its own subsequent turn starts.

This is shared AgentKit presentation state, consumed by the Web host. The host
renders the recovered status but does not infer it from local connection state.
Swift ClientKit remains the separate Swift owner; this change does not alter its
notice model or claim native UI parity. Validation uses the public presentation
API and the Web host renderer, plus the headless consumer build/tests.
