# Pi 子代理与 Native 有界子历史

本轮对齐 ClientKit 的无 UI 行为。ClientKit、Tekes host、Pi/DSH 安装目录保持只读。
Kernel 变更仅为用户批准的只读 `subagents.history` 扩展，不能由版本号 0.9.0 推断能力。
部署固定使用 [Kernel v0.9.1](https://github.com/TekesApps/TekesKernel/tree/v0.9.1) 或其完整 commit；原 v0.9.0 与 AgentKit v0.2.0 tag 保持不变。
本轮扩展固定于 [Kernel 4599331](https://github.com/TekesApps/TekesKernel/commit/4599331f29cb4452b6540131eaee122fc5ca4d6a)，
对应 [Kernel PR #82](https://github.com/TekesApps/TekesKernel/pull/82)。参考来源哈希见
[child-source-baseline.json](child-source-baseline.json)。

```rust,ignore
let child = runtime.create_child(parent, Some("Review"), "stable-create-key").await?;
runtime.prompt_child(parent, &child.session_id, PromptMode::Queue,
    vec![serde_json::json!({"type":"text","text":"Review this change"})], "prompt-rpc").await?;
let result = runtime.read_child(parent, &child.session_id).await?;
let run = ChildRun::open(runtime.clone(), parent_scope, child.session_id,
    ChildHistoryLayout::IndependentSessions, None).await?;
let transcript = run.transcript().await?;
```

CLI 提供 childCreate/childRead/childHistory/childEarlier。Native bounded child 使用 SameSession
和原生 catalog 的 child ID；不把父 journal 当作子历史。

## Pi

create 创建独立空白 session，继承 cwd、workspace 和模型选择。父会话与 request key
绑定持久化回执：同内容重放返回同一 ID，不同 label 冲突；归档或丢失的 child 不会被
同一个 key 替换。普通 fork provenance 不进入子代理目录。

模型侧通过官方 extension API 的 `tekes_subagent` 执行 list/create/prompt/read/cancel。
仅 create 规范化为 Summary subagent 项，其他动作不重复生成导航。宿主桥接通知带
随机 token，过滤出 transcript/native event 输出；超时或 abort 后未知写操作不自动重试。

queue 总是先持久化，steer 走原生 prompt。cancel 清理该 child 的队列和 Goal 调度，
不取消父会话。文件输入拒绝。read 使用公开 get_state/get_entries，含 pendingInputs，
运行状态包含排队工作，回包上限 4 MiB。重连保留原生 pointer，文件丢失明确失败。

## Native

只读接口验证 root spawn 与 child genesis 的双向绑定，复用原生 projector，不启动 worker、
不修复 ledger、不创建 endpoint journal。目前覆盖 root 的直接 child，不寻址嵌套后代。
原版 Kernel 0.9.0 没有该方法，adapter 通过只读探测开启能力；旧版其他 SEP 功能继续可用。

`ChildRun::transcript/load_earlier` 使用 child 自身的序列和冻结 cut/revision。刷新后只有
验证旧前缀未变才保留已加载旧页，历史重写会替换投影。synthetic task-source 来自父工具
有效 invocation，正文只含 child 历史。默认每页 50 messages，服务端最多 200，单 ledger
32 MiB，回包 4 MiB。此接口不增加 bounded child 的续写或单独取消。

## 验证

覆盖 Rust contract/state/adapter 回归、Kernel projector/只读路由与绑定校验，以及真实
已安装 runtime 配合临时状态、本地合成 provider 的集成。Kernel 临时 fixture 显式配置
多个 provider lease，避免父 task 等待 child 时被默认单 lease 阻塞。不改用户全局配置。
这些证据不代表付费模型、原生 UI、性能或跨平台体验验收。

本地结果：AgentKit 76 项 Rust 测试通过；Kernel endpoint/store/supervisor 共 348 项
通过、15 项按原有条件忽略；两库相关 strict Clippy、fmt 和契约/依赖检查通过。
真实集成覆盖上述固定 Kernel、Pi 1.3.0 的 API/模型子代理闭环，以及 DSH 0.2.0-rc.2
的现有 child control/history/reconnect 回归。全部使用临时状态和本地合成 provider。

```sh
python3 scripts/test-real-session-endpoints.py \
  --kernel-root /absolute/kernel-checkout-at-4599331 \
  --kernel-target-dir /absolute/explicit-kernel-build-target \
  --pi-root /absolute/pi-1.3.0-installation
```

Kernel 的 target 参数是显式开发输入；省略时使用该源码 root 下的 target。
