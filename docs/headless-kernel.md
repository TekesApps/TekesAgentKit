# Kernel + MemoryProjectionStore：首个无 UI 闭环

日期：2026-10-17。实现只在 TekesAgentKit；ClientKit、Kernel、Tekes、TekesUI 均只读。

## 当前协议基准

当前 ClientKit `NativeSessionEndpoint` 使用 Kernel protocol **4**，不支持 workspace RPC。
Host description 的通用校验还保留 Pi/DSH v3，不能据此让 Native 接受 v3。
本轮实际对接的是已有 `codex/kernel-090-release` 工作树中的 Kernel 0.9 二进制；
`/Users/frankqin/dev/TekesKernel` 主目录仍是旧版本，不能用来验证此客户端。
精确源码 HEAD/哈希、二进制哈希与来源见 headless-source-baseline.json；编译后的二进制与 source HEAD 分别记录，
不由版本字符串推断二进制必然来自该 HEAD。

协议读取来源包括 ClientKit 的 SessionEndpoint、EndpointV3、JournalV3、History、Inventory、ControlV3、
ActionablesV3、Execution、Event/Registry、NativeSessionEndpoint 及其 Execution 扩展、Runtime、WindowCoordinator、EventProjector。

## 数据路径与 API

```text
Kernel HTTP / WebSocket
  → NativeSessionEndpoint (authentication, mux streams, unary envelope)
  → SessionRuntime (reader ownership, retries, command receipts)
  → MemoryProjectionStore (inventory, control, actionables, journal window, previews)
  → revision notification / inspect / explicit immutable snapshot
```

`EndpointConnector` 每次连接产生独立 endpoint；地址由 host 提供，launcher 和凭证选择属于 host。
示例与真实验收脚本演示私有 bootstrap，但生产库没有隐式启动 Kernel 或搜索用户凭证的行为。

`SessionRuntime::spawn` 后用 `wait_ready` 等待 inventory/control/actionables 基线及 recovery 调用。
`open_session` 打开 journal 并等待 snapshot；`load_earlier` 保持初始 throughSequence；`close_session` 关闭该逻辑流并保留已读历史。
`call` 保留原 ClientKit unary 信封及显式 rpcId，支持首批外的 Kernel 方法而不伪造已实现的强类型 API。
`create_session` 提供 cwd，由 Native adapter 转换为无环境授权的最小 execution；更完整授权由 host 用 `call` 显式提供。
`prompt` 是 queue/text 便利方法，`cancel`、`respond` 分别对应取消与 actionable 回复。

## 行为对齐与保留项

| 项目 | 本轮实现 |
|---|---|
| 连接 | v4 原生握手、必需能力检查、8 MiB 消息上限、bearer header；不记录凭证 |
| 流 | inventory、control、actionables 与按需 session-journal；基线先于 delta；连接 generation 校验 |
| Inventory | baseline/upsert/remove/status/plane/agent-error，保留 opaque summary 扩展字段 |
| Control | queue/jobs 全替换，projection 按键保留最高 sequence；缺失字段与显式 null 区分 |
| 标题兼容 | 与 Native 相同，仅 inventory/control 的 legacy sessionTitle 规范化；不重写未知 journal 事件数据 |
| Journal | snapshot 校验、连续 live cursor、重复内容冲突检测、未知必需事件拒绝、可忽略未知事件保留 |
| History | frozen cut、session/cut/cursor 校验、旧页尾连续性、historyRevision 变化后丢弃旧覆盖 |
| Rebaseline | 有连接覆盖时保留早期已载历史；snapshot 不相邻时重建 raw window；局部页不宣称全历史删除权威 |
| Transient | 不推进耐久 cursor；按 turn/step 聚合 text/reasoning preview；权威 message/step/end/turn/end 或断线清理 |
| Actionables | revision 校验，过期回复拦截；请求成功不自行删除，等待权威 resolved/upsert；断线清除操作权威 |
| Mutation | unary 关联 rpcId；不自动重试不确定 mutation；HTTP 错误保留远端错误结构 |
| 恢复 | 单 journal 首先重新开流，连续失败上限 5；连接重试有退避和上限，失败后可手动 reconnect |
| 通知 | 增量写入短锁内存状态，revision 通知不携带历史；仅显式 snapshot 复制完整状态 |
| 无落盘 | 无 SQLite/Mirror/cache；Kernel 自身权威 ledger 的持久化保持原样 |

上述范围是 headless 同步契约对齐，不是 UI parity 声明。MemoryProjectionStore 保存完整事件及扩展值，
只折叠首批 run/preview 控制事实；ClientKit 的 surface 替换/工具/任务/usage 等完整 UI projector 尚未移植。
因此 entries 是未来 presentation 的输入，不能把它当作已完成的视觉 transcript。

## 验证

- Rust：contract fixture、投影、runtime 与真实 HTTP/WS 协议对端测试；包括断线自动恢复、重开、错误关联、旧流迟到帧、gap/冲突、分页、transient 与 stale actionable。
- Rust 契约回归：保留 **22 个 JSON 用例**。实现阶段曾与外部 ClientKit 校验过相同用例，作为本次基准证据；
  AgentKit 仓库已移除 Swift 校验源码及其编译脚本，后续构建/测试只运行 Rust 用例。外部 ClientKit 改动不会自动刷新这些用例，需重新核对来源。
  用例是本轮依据当前契约合成，非宣称覆盖全部官方 fixture；包含整数数值拼写、缺失/null 与未知事件等边界。
- 真实 Kernel 0.9：临时 HOME/状态目录、私有 bootstrap、随机 endpoint token、本地合成 provider。
  connect → empty inventory → create(execution) → selectModel → follow → prompt → durable assistant/turn end → same-rpcId retry → reconnect/reopen → frozen history page → cancel → second turn end → stop。
  首个 prompt 重试后仍只有一条 durable user/message。取消可在 provider dispatch 前生效；本次仅首个 prompt 到达合成 provider。
- headless 依赖检查：contract/session/三个 adapter/headless 不依赖 UI、SQLite、Kernel server 内部 crate 或隐式 sibling path。
- fmt、Clippy `-D warnings`、workspace tests 均作为本地验收；新增 CI 配置不等于云 CI 已通过。

复现命令见 README。真实 Kernel 依赖 host 已构建的明确 root；脚本不修改/重编译该源码工作树。

## 当前限制

- 没有 UI、durable cache、附件等完整强类型客户端 API，也没有生产 launcher。Pi/DSH 已有独立实现，见 session-endpoints.md。
- 流缓冲为每流 256 帧，溢出关闭连接并重建基线；不同调用方的背压/吞吐还需实际数据验证。
- 连续失败预算是有限的保护，尚未实现完整的重试策略配置与错误分类 UI；连接健康 30 秒后重置累计连接失败预算。
- runtime 的 unary/回答并发运行，不阻塞事件投影；分页仍由 command owner 串行执行，底层 transport 保持有界缓冲。
- 已加载历史保存在内存，无自动淘汰；完整 snapshot 复制与显式历史重折叠仍有成本。未测大历史、RSS、真实 provider 或性能收益。
- 不支持仅提供 projected-stream 而没有 session-journal 的 Native host。
- 当前验收是 macOS 本地测试；Linux CI、TLS 远端、真机和生产均未验证。
