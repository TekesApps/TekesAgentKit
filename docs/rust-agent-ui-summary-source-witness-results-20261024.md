# #25：原 Summary 来源见证与真实 Pi 消费结果

本提交交付显式验收 feature 和只读 CPU consumer，关联 [#25](https://github.com/TekesApps/TekesAgentKit/issues/25)、#27、[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。[契约](rust-agent-ui-summary-source-witness-contract-20261024.md) · [机器收据](rust-agent-ui-summary-source-witness-results-20261024.json)。UI 继续试验；#25 不关闭，不合并/tag/发布。

## 实现与行为

- 原 RunController 的刷新/发布边界返回实际发布的 Arc<RunSnapshot>、同一个 runtime.inspect 内存输入及成功标志。字段私有、getter只读；正常路径不复制 authority。失败沿原 degraded 行为保留旧内容，成功标志为 false。没有增加新的 Summary 状态机或算法。
- `acceptance-source-witness` 默认关闭；headless 的 `presentationSource` 只在显式开启时可用，未接 Web API/生产 UI。CI 同时保留默认检查并增加 feature 检查。
- 隔离真实 Pi1.3.0 + 合成 loopback provider：原模型工具创建 Model child，通过原 Summary pointer 打开 child、公开等待其输入结算、读取 childHistory，重连后重新捕获原 Summary。12条公开命令、5次provider请求；程序 SHA256 `2ebad9784377de4e55a890df6478db2ac22a8b16c06fca2a988c4ab37d7ac022`。没有付费模型、私库读写或桌面操作。
- 最终同帧父/子 ID、原工具结果 entryId/ordinal、同端已知子 inventory 与 Summary 行一致。Pi 返回的是 NativeConversation 位置，不能当 journal seq 或从 opaque rowId 拆 callId。childHistory 含子输入，不含父输入；重连 epoch 增长且 pointer 保持。
- 子 inventory **没有显式 raw status**；原规则由非运行/非空、无排队等公开输入给出 `idle`，不能补写 `completed`。原 Summary 的 indicator 为 None。只证明这个已观察到的 idle 范围，不证明 running/failure 转换或服务端多流事务一致。
- 第二 public consumer example 将这份原 Snapshot.summary 原样交给 SummaryView→ContentHost；action row、标题、scope、原 facts 与 indicator policy 一致。只挂载，不渲染，不宣称 glyph/像素/hover/真实窗口。

## 验证与证据

默认根测试341/0；开启feature的presentation测试249/0（包含两项新的公开协议来源测试）；Python189/0（新增12项来源/拒绝测试）；feature严格Clippy与UI严格Clippy通过。新增consumer JSON dev依赖已显式加入精确边界白名单，静态边界复验通过；不增加平台/renderer/harness依赖。UI全量747/0/1原有忽略通过；独立外部consumer六次公开挂载实际执行通过，新的源码闭包见机器收据。协议fixture额外确认原发布Arc、旧见证不可变、跨epoch、同端候选和 malformed/degraded 不伪装为新鲜来源。

证据在 SSD `.runtime-fixtures/summary-source-witness-20261024/`：原stdout/stderr、commands、程序SHA、source输入、原witness、component收据与检查日志。首次Pi捕获仍是notStarted，没有被当终态；随后一次校验误把缺raw status当作与idle矛盾，失败日志完整保留，修正验收假设后重跑。部分最初临时程序未在覆盖前另行归档，只有SHA/原输出；最终feature程序已单独归档，历史目录只归档不再运行。

## 仍未验收

真实running/failure转换、此profile父草稿隔离、完整最终UI/窄宽字号主题、glyph/hover/selection、物理IME、OS AX、桌面和ProcessContainer均不能借用本切片通过。已核实6e2a016的PR/push CI仅严格#53失败；本提交的新CI尚待云端运行，不能借用旧CI。TekesUI #53仍OPEN；#47等基础组件缺口继续由上游负责。只消费固定公开50ec49f，不修改TekesUI/Kernel/ClientKit/Tekes；只读参考的两仓库WIP保留。所有构建复用SSD共享target。结束后在原target/debug/.cargo-lock上使用单一非阻塞flock，检查进程与lsof，删除本轮最初debug验证产生的4个可再生incremental目录；du分配占用实际减少205,983,744字节（约196.4MiB），不宣称APFS独占物理extent释放。共享deps、固定快照、WIP和程序保留；另一个TekesUI worktree的Cargo进程未干预。
