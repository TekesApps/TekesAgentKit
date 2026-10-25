# AgentKit 阶段协议验收矩阵（2026-10-25）

本矩阵只覆盖收口阶段的 #55 Run 和 #56 session/history/family 协议验收。交付候选固定为 AgentKit `5b7c84361206b45ac381b10437097d3b76c322c1`；行为参照 ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd`；UI 消费 pin 为 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。不纳入候选之后的 dirty WIP、桌面/物理 IME/OS AX 或 #60–#66。固定运行时和二进制身份见[协议证据审计](agentkit-stage-protocol-evidence-audit-20261025.md)。

所有可复用证据必须同时匹配源码影响面、程序 SHA、运行时身份及该行的行为断言。不能匹配时只重跑该行；不得把旧提交的通过转授给 5b。失败、未运行和无能力是不同结果。`Unsupported` 仅用于固定运行时公开契约明示不支持且随收据保存契约依据的原子能力；Kernel 0.9.3 busy queue 明确属于受支持而失败，不能改列 Unsupported。

## 有限矩阵

| 端点 / 固定运行时 | 本阶段能力与证据 | 5b 状态 | 最小剩余验收 |
|---|---|---|---|
| Native / Kernel 0.9.3 v4 | mixed text/image/file、send/stream、busy queue、cancel/recover、history/page/reconnect、family。历史真实综合证据：`docs/rust-agent-ui-native-dsh-busy-queue-results-20261025.json`（来源 29ea2e1，Native busy 队列为空，失败）；Native 分页 `docs/rust-agent-ui-native-paged-live-results-20261025.json`（b0fa6e5）；5b warm bounded history/passive Summary `.runtime-fixtures/acceptance-refresh-20261025/native-summary-transition-live-3-warm/accepted.json`。运行时 supervisor/worker 和 provider-config SHA 见审计文档。 | **阻塞/部分可复用**。Kernel #86 busy queue 为必需真实失败；warm 分页/被动 Summary 仅窄通过，不是 formal Task、parent failure、cold-start、阅读锚点或全 family 通过。 | 不重跑已有 5b warm 分页/被动 Summary。保留 Kernel busy queue 红证据并等待其公开契约可通过的固定运行时；若保持 0.9.3，#55 不通过。只做一次 5b 离屏 integration/source check，确认已有分页收据与当前 demo 的绑定及 page plan 对应。若检查发现来源回传缺陷，回实现票修复后仅重验受影响行。 |
| Pi / 1.3.0，Node 24.20.0 | mixed direct/queued attachment、公开历史、send/stream/queue/reconnect。`docs/rust-agent-ui-pi-mixed-file-results-20261025.md` 和 JSON 是真实通过，但绑定 06c0908；CLI SHA `e79626f2dd6f94aa45d30f3fa63cd84319a6eefcd150b353cfaf274366926774`。 | **历史语义已证，5b Run 未证**：Pi endpoint/composer/presentation 在 06c0908 到 5b 间变化。 | 用冻结 5b 源码和同一固定 Pi/Node，只跑一次隔离 Run workflow：direct + queued mixed text/image/file，核对原 input receipt / queue fact、公开历史 bytes/order、流式工具/推理/终态、重连及请求计数。保留原始帧、provider 观测和退出收据；不把文件路径提示说成模型读取文件正文。 |
| DSH / 0.2.1-rc.2 | busy queue/consume/reconnect 旧真实通过见 `docs/rust-agent-ui-native-dsh-busy-queue-results-20261025.json`（29ea2e1）；入口 SHA `80112efe368efb1a39720c7ada0af00442e5131fb2744edaaff8a3e8bb499e6f`。 | **5b Run 未证**：DSH queue adapter 在旧通过后有改动。 | 用冻结 5b 源码和固定 DSH 只跑一次相同范围的 busy/consume/reconnect Run workflow；逐项检查原公开 queue fact、source/request identity、输入和最终回传、无重复 provider mutation。 |
| Native / Pi / DSH history | 三端 recovery 来源可参考 `docs/rust-agent-ui-sidebar-transport-recovery-results-20261025.json`（29ea2e1，三端 epoch 2→11→13）。Native 另有 b0fa6e5 分页和 5b warm proof。 | recovery 语义证据有效但需按 receipt 中冻结输入核对其变更影响；不能单独代表 5b 端到端历史。Native 窄范围已有 5b 证据。 | 仅检查 frozen manifest 里 history/pointer/reconnect 调用链自历史收据后的变更。如对应路径未变则复用原 receipt；若变，只对受影响端跑一次 pointer→公开 history→Refresh/Earlier→reconnect 的来源绑定检查。检查失败才运行时重试。 |
| Native / Pi / DSH family | 旧真实 producer 分类/生命周期来源 `docs/rust-agent-ui-real-task-family-results-20261024.json`（f399356）；三端真实同进程控件/数据分发见同文及 `.runtime-fixtures/real-multi-sidebar-20261024/family-host-verified/`。Native 5b passive Summary 只覆盖 Summary 状态投影。 | **旧源证据可复用为契约/producer依据；5b 三端 family 未闭环**。不能由字段缺失推导 Unsupported；Native passive Summary 不等于 Task lifecycle 或 parent failure。 | 对照 5b 受影响 family/binding 路径。如调用/投影路径有变，只对各端实际可用 producer 做一次原 producer→原数据回传→原 source retirement 检查。契约无该 producer 的单项，保存公开 capability 声明后才标 Unsupported；没有 producer 的结果记为未验收，不伪造源。 |

## 固定能力边界（不是通过声明）

依据冻结5b的 `docs/session-endpoints.md`、endpoint capability响应及原收据。握手与文档不符时记失败/待查，不用缺字段自动判Unsupported。

| 单项 | Native 0.9.3 | Pi 1.3.0 | DSH 0.2.1-rc.2 |
| --- | --- | --- | --- |
| 发送/取消/流式 | 原生unary/rpcId及durable/preview；均适用 | text/image/file、abort、native event；均适用 | prompt/cancel、durable/assistant-stream；均适用 |
| 附件与busy队列 | 支持；busy queue当前Kernel #86失败 | 持久化queue/steer、公开receipt；适用 | raw upload及inbox queue；适用 |
| 通用历史附件字节读取 | 公开session内读取适用 | session隔离公开fileAttachment适用 | 图片公开读取；通用文件下载无公开能力，不伪造下载通过；发送及元数据仍须验 |
| pointer/恢复/分页 | inventory、journal frozen-cut、throughSequence | 宿主pointer、授权根内adopt，经get_state/get_entries恢复/ancestry窗口 | inventory、follow/page、throughSeq |
| 重连与源退休 | 适用 | 适用，不自动重发不确定提交 | 适用 |
| 临时SideChat fork/discard | 原生普通/临时fork、discard适用 | 原生clone/entry-fork及临时discard适用 | 仅普通fork；正式temporary fork/discard明确Unsupported，保留禁用路径验证 |
| 子代理 | 原生catalog和只读bounded history；bounded child控制不具能力 | 公开create/list/prompt/read/cancel | 公开projection catalog、父拥有prompt/cancel和child history |
| Task/Summary来源 | passive tool来源已有窄见证；正式Task/父raw result失败尚未验收，不能据子错误判父失败 | 不伪造journal/task/job；实际合法producer与passive来源分别核验 | 实际公开producer和原source分别核验；不能从工具文本猜family |

## 非桌面性能门槛

#55 与 #56 的现有关闭标准要求性能先冻结、再实测。开始任何性能命令前，主会话必须在 Issue/收据中冻结一个有限的可比较 profile：数据集规模和内容摘要；机器/OS/电源模式；精确程序与运行时；cold/warm 定义；每组迭代次数；延迟 p50/p95；峰值 RSS；恢复收敛的起止事件与预算；可比较基线及阈值。随后每端按该 profile 各跑一次固定 workload，保留原始样本。主会话随后冻结了[有限性能 profile](agentkit-stage-performance-profile-20261025.json)：2 sessions、每会话100×1024-byte历史/10工具组、20条分页；cold-process 5次、warm 10次，p95绝对预算与相对main基线预算、采样峰值RSS及cleanup预算均写入JSON。它是测试前门槛，不是性能结果；机器/电源和基线样本尚待同机绑定，性能状态仍是**未验收**；不得在看到结果后调整阈值，也不得以桌面/IME 未跑替代性能验收。

性能以外的矩阵不得扩成新 ClientKit 变化清单。运行环境身份变化时创建新的收据并标明不同版本，不改写本矩阵基线。

## 主会话的最小执行顺序

1. 在隔离冻结输入上串行执行 Pi 和 DSH 各一次 Run workflow；不要在带 WIP 的主 checkout 编译。每次保存 source/lock manifest、程序 SHA、运行时握手、原始 SEP 帧/provider 记录及 owned-process exit。
2. 核对三端历史/pointer/family 变更影响面；复用匹配收据，只运行缺失或受影响的原子行。Native busy queue 保持红色阻塞，除非固定公开运行时取得满足 Kernel #86 契约的新证据。
3. 先冻结上述性能 profile，再各端运行一次；profile 未冻结前不要启动 benchmark。
4. 任一失败回归对应实现 Issue。仅重跑修复影响的矩阵行，随后绑定最终候选收据；桌面/物理 IME/AX 保持 `not_run`。

本矩阵定义有限验收面，不自动关闭 #55/#56，不把任一端的 capability 复制到其他端，也不减少 Epic #14 的完整要求。

## 本次执行结果（冻结5b，限定范围）

[程序/来源/收据绑定](agentkit-stage-protocol-results-20261025.json)：1207项冻结输入前后未变，扩展WIP未进入批次。主会话串行完成两个probe定向构建（27.84s），复用SSD target；之后未重新全量构建。

- Pi `pi-run-5b/accepted-receipt.json`：direct/queued mixed附件、公开历史与file读取、重连来源均通过；3次provider请求，无自动重试，epoch 2→5；headless/probe均exit0，无owned descendants，provider停服。
- DSH首次 `dsh-run-5b/failure.json`：固定60秒readiness超时，尚未进入队列workflow。失败和清退证据保留，不覆盖。相同输入/版本/超时、另一个隔离目录的显式诊断复验 `dsh-run-5b-retry/run-receipt.json` 通过busy queue/consume、mixed附件与历史重连；3次业务请求加1次标题请求，Native/DSH均exit0。该成功不证明冷启动稳定；超时原因未定位，观察到Node处于U状态不能据此断言具体I/O原因。
- Native原warm3仍仅按其原窄范围复用；Kernel #86、取消/工具推理全矩阵、完整三端Family/父同authority失败及性能等缺口未因此通过。未启动桌面/IME/AX。

阶段仍未具备合并/发布全部候选的条件。原程序已归档到本阶段证据目录，仅作证据，不创建/运行新桌面应用身份。未清理缓存，释放0字节。

Pi Summary补验：`pi-summary-5b/run-receipt.json` accepted，真实模型子代理公开历史/新epoch pointer及同一次MemoryProjection publication的Summary来源通过。状态为idle，依据public-idle-inputs-no-explicit-child-status；不证明正式Task completed、running transition、父draft隔离或服务端事务新鲜性。

DSH Summary补验：`dsh-summary-5b` 在同一60秒readiness门槛超时，未进入投影，owned进程已退出。记为本次未通过，不把旧Summary程序结果移给5b，也不据启动失败断言投影产品bug；#56继续保留该缺口。未继续循环重试。
