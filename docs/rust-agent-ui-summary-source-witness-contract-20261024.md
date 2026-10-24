# #25：原 Summary 构建边界的只读来源见证（验收专用 feature）

关联 [#25](https://github.com/TekesApps/TekesAgentKit/issues/25)、#27。只改 AgentKit，SDK/Kernel/ClientKit/Tekes只读；无桌面/物理IME/AX。本切片不是Summary图标/视觉等剩余工作的替代。

## 问题与决定

真实Pi model-subagent流程已读取原Summary的标题/pointer，但未精确核验子行状态所用的原始来源。分别读取runtime snapshot和presentation不能拼为同帧证明；watch revision在store锁释放后递增，也不能借它推造原子来源身份。双读相等最多证明观察到的稳定终态，排除不了ABA。

经只读advisor审阅，使用原RunController刷新/发布边界的一次性见证。显式 `acceptance-source-witness` feature下提供 `RunController::refresh_with_source_witness()`，返回实际发布的原 `Arc<RunSnapshot>`、同一runtime.inspect闭包内克隆的原 `MemoryProjectionStore` 与 `projection_succeeded`。见证字段私有，仅提供不可变getter，调用者不能重组或改写原配对。正常构建路径不复制authority，不增加IO/权限/网络方法，不使用持锁任意回调，不另建PresentationStore或Summary算法。保持原 core Mutex→runtime.inspect 锁顺序。

Headless同名opt-in feature提供 `presentationSource` 验收命令；默认构建不支持，不能静默回落为两次独立读取。只从原projection读取，不读harness私库。原完整内存store作为输入见证避免遗漏Summary fallback所需的summary/run_open/queue/actionables/保留turn start/end与readiness；仅在显式验收操作复制/输出，不灌入组件。

原刷新失败沿现有degraded路径发布旧内容，见证必须 `projection_succeeded=false`；调用者不能把新authority贴旧Summary当成功。核心Summary内部不可用时原None/diagnostic同样保留，真实验收必须拒绝把缺行当成功。

## 验收范围

1. Feature关闭时默认行为、API/命令边界不变；开启后见证中的snapshot即原发布对象，原revision/selection/epoch与source对应。
2. 一次capture后来源变化不重写旧见证；跨端/epoch/readiness、malformed refresh/degraded等负例不被标为新鲜成功；不依赖后来runtime读数证明旧帧。
3. 原Pi公开模型工具生成真实child，从真实父tool result与同端候选集合建立Summary关系；不以测试预期ID替换producer解析结果。
4. 若能受控保持child运行，则验证原Summary running/indicator到公开终态/无running indicator；若只有终态，仅交付终态范围，running/failure保持未验收。
5. 原Summary组件消费该原Snapshot；父Composer/历史和pointer权限不改变，见证不授予导航权。

这证明内存投影构建来源的一致性，不证明服务端多网络流事务一致，也不证明捕获全部瞬态。feature专用检查及实际无窗口来源证据分别记录，未实现/未验证项不勾选。复用SSD共享target，保留来源、程序和失败证据。实际结果见[本轮收据](rust-agent-ui-summary-source-witness-results-20261024.md)。本切片不能作为 #25 整体关闭依据。

## 已实现的公开入口

```sh
cargo test -p tekes-agent-presentation --locked --features acceptance-source-witness
cargo build -p tekes-agent-headless --locked --features acceptance-source-witness
```

`presentationSource` 与既有 `presentation` 一样选择原 Run，返回 `{snapshot, authority, projectionSucceeded, evidenceScope, serverTransactionalFreshnessProven:false}`。可显式 `saveAs` + `savePointer` 保存该原返回值里的子 pointer，供同一批次后续公开 `open` / `childHistory` 命令使用；未开启 feature 时没有此命令。返回的 authority 使用其公开 Rust 序列化字段名（snake_case），RunSnapshot 使用既有 camelCase，不重命名来源。此命令只供隔离验收，未接 Web API，也不是生产 UI 的数据接口。

真实 Pi runner `scripts/test-real-pi-summary-source.py` 只接收已构建程序和固定 Pi 安装，不调用 Cargo。它从原模型工具创建子会话，公开等待子输入结算、读取 childHistory、重连，再在同一次原 publication 中捕获父 Summary 和来源。Pi 的 resultPosition 是原 NativeConversation entryId/ordinal，不能当 Journal seq，也不能拆 opaque rowId 获取 callId。校验器按原位置核对原 message，严格区分显式子 status 与本轮已观察的 idle fallback。其他 fallback 不在该验收 profile 内。

公开 consumer example `summary_source` 只把见证中的原 Snapshot.summary 交给 SummaryView，挂载 ContentHost 并核对原 action row/title/scope/indicator policy；没有重建 Summary 算法，也没有渲染/窗口。只证明挂载和指标语义，glyph/hover/布局仍未验收。
