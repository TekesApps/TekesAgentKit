# TekesAgentKit

与 TekesClientKit 对齐的 Rust Agent 客户端套件。

**当前：Kernel protocol v4 + 内存投影的首个无 UI 闭环。** Pi、DSH、Presentation 和 Components 仍是骨架；没有接入 TekesUI，没有客户端 SQLite/Mirror 持久化。

- `tekes-agent-contract`：headless endpoint trait、握手、journal/history DTO、事件验证与共享 fixture。
- `tekes-agent-endpoint-kernel`：bearer 认证 HTTP unary / WebSocket mux，逻辑流生命周期、有限缓冲和请求关联。
- `tekes-agent-session`：inventory/control/actionables、按需 journal、内存窗口与 preview、冻结切点分页、恢复和提交意图。

当前 ClientKit 的 Native adapter 要求 Kernel v4，已移除 workspace RPC。Kernel 旧 checkout 的 v3 协议不能作为当前基准。
本轮对齐的是 headless 同步和命令路径，不等同于全部 ClientKit API 或其 UI projector 已移植。

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/check-headless-boundaries.py
```

当前 Swift ClientKit 的同用例校验、隔离真实 Kernel 验证：

```sh
python3 scripts/check-clientkit-parity.py --clientkit-root /absolute/TekesClientKit
python3 scripts/test-real-kernel.py --kernel-root /absolute/compatible-kernel-worktree
```

后者要求该 Kernel root 已构建 supervisor/worker/helper/workspace-service；使用临时 HOME、状态目录和本地合成 provider，不访问真实模型服务。

公共 API 示例：

- [kernel_headless](crates/endpoint-kernel/examples/kernel_headless.rs)：连接、建会话、选模型、提交、幂等重试、重连、取消和停止；连接凭证从 stdin 接收。
- [replay](crates/session/examples/replay.rs)：从 NDJSON 回放四类流，输出最终内存读模型。

`SessionRuntime::inspect` 与 subscription 的 revision 通知适合常规读取；`snapshot()` 是调用方显式请求的完整不可变副本。
运行时的同步锁不跨越网络 await。mutations 不自动重试；调用方应保存原 rpcId 并在不确定结果时先核对权威状态。

参见 [实现与验收边界](docs/headless-kernel.md)、[开发路线](docs/roadmap.md)、[本轮源码基准](docs/headless-source-baseline.json)。
最初的 [架构评估](docs/assessment.md) 保留为历史记录，其中旧 Kernel v3/workspace 描述已由本轮 v4 核对纠正。

Rust 工具链固定 1.95.0；所有 crate 暂不发布，尚未选定开源许可证。开发覆盖必须显式指定；发布依赖须使用已发布精确版本或不可变 Git revision。
