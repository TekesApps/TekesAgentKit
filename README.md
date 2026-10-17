# TekesAgentKit

与 TekesClientKit 对齐的 Rust Agent 客户端套件。仓库没有 Swift 源码，构建与测试不依赖 Swift 工具链。

当前实现三个独立的 SessionEndpoint，加共享的内存投影和无 UI runtime：

| Endpoint | 当前支持版本 | 权威数据路径 |
|---|---|---|
| `NativeSessionEndpoint` | Kernel **0.9.0** / SessionEndpoint **v4** | bearer HTTP unary、WebSocket mux、journal / frozen-cut history |
| `PiSessionEndpoint` | Pi **1.3.0** | 原生 stdio RPC、native session tree 当前分支 |
| `DshSessionEndpoint` | DSH **0.2.0-rc.2** | 认证 Remote HTTP / WebSocket mux、journal / page、multipart 文件 |

三个 adapter 分别拥有协议、生命周期和历史读取方式；共享层不会把 Pi 原生 entry ID 伪造成 journal sequence。
`SessionEndpointRegistry` 使用 `endpointId + sessionId` 路由；`SessionRuntime` 处理 inventory、control、actionables、按需会话窗口、重连和显式 rpcId。
`SessionLocator` 返回宿主位置、原生 session ID / 文件指针及历史读取来源。远端路径不会回退为本机读盘。

本地可以直接使用 `MemoryProjectionStore`，**不需要客户端 Mirror**。当前三个 endpoint 都走内存投影；远端 durable cache 尚未实现。
Pi 的宿主持久化指针注册表和保守的提交 receipt 用于恢复及避免重复提交，并不是历史 Mirror。
Presentation / Components 仍是骨架，没有接 TekesUI。当前是 headless 核心闭环，尚未覆盖 ClientKit 的全部可选 API。

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/check-headless-boundaries.py
```

三个真实运行时的隔离验收，可任选一个或多个 root：

```sh
python3 scripts/test-real-session-endpoints.py \
  --kernel-root /absolute/compatible-kernel-0.9-worktree \
  --pi-root /absolute/pi-1.3.0-installation \
  --dsh-root /absolute/dsh-0.2.0-rc.2-installation
```

脚本使用临时 HOME、状态目录和本地合成 provider，不请求真实模型服务，不修改运行时源码。Kernel root 须已有 supervisor/worker/helper/workspace-service；Pi/DSH root 须有安装好的 Node 和对应 CLI。
另有 `scripts/test-real-kernel.py` 验证 Kernel 的 frozen-cut 分页和取消结算。

`tekes-agent-headless` 从 stdin JSON 接收连接配置和批处理命令；凭证不放在 argv。覆盖建会话、选模型、提交、回传、交互回答、重连、定位、历史和文件读取。
公共 API 示例：[kernel_headless](crates/endpoint-kernel/examples/kernel_headless.rs)、[replay](crates/session/examples/replay.rs)。
常规读取使用 `inspect` 和 revision subscription，显式 `snapshot()` 才复制完整读模型。慢 unary 不阻塞事件投影；不自动重试不确定 mutation。

详见 [三个 Endpoint 的能力与验收](docs/session-endpoints.md)、[Kernel 同步契约](docs/headless-kernel.md)、[路线](docs/roadmap.md)、[源码与运行时基准](docs/headless-source-baseline.json)。
最初的 [架构评估](docs/assessment.md) 保留为历史记录，旧 Kernel v3/workspace 描述已由 v4 基准纠正。

Rust 工具链固定 1.95.0，crate 暂不发布，尚未选定开源许可证。开发覆盖须显式指定，发布依赖须使用已发布精确版本或不可变 Git revision。
