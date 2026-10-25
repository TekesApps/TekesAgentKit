# #55/#56 阶段协议证据审计（2026-10-25）

只读审计；不改变 Issue、验收矩阵、源码或既有收据。建议冻结交付候选为 AgentKit `5b7c84361206b45ac381b10437097d3b76c322c1`，ClientKit 行为参照 `c289bc9d33edcd83a782ac17927a8c65a027eccd`，TekesUI 精确 pin `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。候选源码之后的 dirty WIP 不属于该基线；它不会被下列证据覆盖。旧的 91129d7 矩阵、29ea2e1/06c0908 live receipts、f399356 #29 见证均保留原身份，不升级为 5b 通过。

## 固定运行时

| 端点 | 本阶段版本 | 身份来源 |
|---|---|---|
| Native | Kernel 0.9.3 / v4 | `09508a9083c24c7accefc99516739f279b85913e`；supervisor SHA256 `365eca9385f36d5897a81ba398e7f34b4b9819896d6a1b78c618a7710b14380f`；worker SHA256 `1284014bd0a8d3cb711c0512b10762322a19af4beabc1f9e681bacb1e28fa64d` |
| Pi | 1.3.0 | 固定 Node 24.20.0；CLI SHA256 `e79626f2dd6f94aa45d30f3fa63cd84319a6eefcd150b353cfaf274366926774`；见 sidebar recovery 结构化收据 |
| DSH | 0.2.1-rc.2 | 固定入口 SHA256 `80112efe368efb1a39720c7ada0af00442e5131fb2744edaaff8a3e8bb499e6f`；见 sidebar recovery 结构化收据 |

不在本切片升级 harness 或 SDK；Native provider 配置来自固定 Kernel 0.9.3 contract root，SHA256 `0caac12724fd6d982fc51123ea4546f2a023d6a9258da4cde63d1d27eabcaa6a`。真实版本以程序/握手收据为准，来源源码或目录名不能替代运行时身份。

## 有限验收切片

每个真实 harness 只执行下表各一组端到端 workflow；内部 assertions 属于该组，不另拆票。静态/fixture/UI 离屏仍是分层证据，不能冒充真实协议。桌面、物理 IME、OS AX 不在本期。

| Issue | 三端各自的有限 workflow | 接受标准 |
|---|---|---|
| #55 Run | 同一隔离 session 发送一次 mixed text/image/file 输入；核对原 submission identity、公开历史字节/顺序及流式 tool/reasoning/最终回传。流 hold 时走各端明确支持的 queue 路径，核对原队列可见、消费、回传；取消另一个真实 hold，核对取消终态、显式恢复且没有自动重放。每组最终显式 reconnect 一次并核对已结算历史/请求计数。 | 原 RPC/request identity 与来源贯穿 prepare/admit/commit；输入 bytes/顺序与公开事实一致；流式与终态可见；queue 只在公开能力支持时要求原 control/inbox 见证；cancel 与恢复不重放 mutation；所有 owned processes 退出后接受。缺任一原始公开见证为失败/未通过，不以 ack 推定完成。 |
| #56 Session/history | 从每端公开 inventory/pointer 选取原会话，验证 locator/adopt（仅 Pi 有 adopt）、只读文件来源；同一来源获取 history，执行一次实际 `Refresh` + `Earlier`，校验公开 page plan、历史身份、epoch 和错误/前缀语义；重连后旧来源请求被拒绝、新来源恢复，父草稿/选中 scope 不串线。 | 不读 harness 私库；Native file pointer 只作定位证据；Earlier 与公开 page plan 一致；reconnect 不换 session identity、不接受旧 epoch；保留有权威来源的文件与 transcript 内容。 |
| #56 Families | 每端各取实际可用的公开 producer/endpoint capability，回传 task/subagent、SideChat、Summary 来源事实；按 parent/origin/ephemeral 等字段原样验证身份、读回与源退休。只对公开契约没有的单项标 Unsupported，并保存能力描述/源码依据。 | 不从工具文本猜 family，不用“字段缺失”证明 main；原 producer/call/source identity 可追溯；失败与无 producer分开记录。Native passive Summary 不替代正式 Task 生命周期或 parent failure。 |

## 可复用证据与当前缺口

| 证据 | 精确身份及可复用范围 | 对 5b 的处理 |
|---|---|---|
| 原 #29 三端分类见证 | `f399356`，Native 0.9.3 / Pi 1.3.0 / DSH 0.2.1-rc.2；真实公开分类 producer 与 15 个生命周期样本。Issue 曾关闭但因未集成重开。 | 仅保留真实 producer/字段语义历史；不能代替 5b Family workflow 或 main 集成。 |
| Pi mixed direct/queued files | `06c0908` Pi 1.3.0；`docs/rust-agent-ui-pi-mixed-file-results-20261025.md` 及 `docs/evidence-bindings-20261025/pi-mixed-06c0908.json`。真实 input receipt、队列、历史、重连。 | Pi endpoint/composer/presentation 路径在该提交到 5b 间变化；不接受为 5b 完整 Run 通过。保留为协议语义与 seed 依据，需针对 5b 重跑有限 Run workflow。 |
| DSH busy queue + Native busy 失败 | `29ea2e1`、程序 SHA `fc44d6dd875e2a7b4053b875fbec561c38ce4b0f21d3906b14901e0ffcbd0638`，runtime DSH 0.2.1-rc.2 / Native 0.9.3；收据 `rust-agent-ui-native-dsh-busy-queue-results-20261025.json`。DSH busy/consume/reconnect 通过；Native admitted 后两份公开 control 都持续空，严格失败。 | DSH queue adapter 在原证明至 5b 之间变化，旧成功不能覆盖候选，需一次定向 DSH Run workflow。Native 不是 Unsupported：Kernel #86 仍 open，公开 v0.9.3 contract 要求完整未消费 queue snapshot/item/source；候选对该门槛阻塞，除非本期固定 runtime 能提供符合契约的新证据。 |
| 三端 transport recovery | `29ea2e1` 基线、584 项来源冻结；真实 Native 0.9.3 / Pi 1.3.0 / DSH 0.2.1-rc.2；每端 epoch `2→11→13`、九行与六份草稿保留、原重连后新库存/历史恢复；程序/版本 SHA 在 `rust-agent-ui-sidebar-transport-recovery-results-20261025.json`。 | 可复用“关闭→有限拒连→显式原宿主重连”协议路径。提交以后须核对 5b 变更文件与冻结 manifest；它不能代替该候选上发送/附件/历史端到端重连断言。 |
| Native bounded history / passive Summary | Native warm proof `.runtime-fixtures/acceptance-refresh-20261025/native-summary-transition-live-3-warm/accepted.json`，候选 5b、Kernel 0.9.3。公开两页 plan，一次 Earlier 至 EOF，重连后旧页与来源保留；passive Summary raw `doing` → canonical `in_progress` → raw `done` / canonical `completed`，Row/View/Title/sourceCall ID 稳定。offscreen source witness，不是桌面。 | 可直接复用其狭义 Native 分页及 passive Summary transition。明确不证明正式 Task 生命周期、parent failure、cold startup、阅读锚点/桌面、完整三端 Family。对应 failed run2 的 30 秒 provider gate timeout 保留，不能被 warm pass 覆盖或改写。 |
| Native bounded child history | `b0fa6e5`、Kernel 0.9.3；`rust-agent-ui-native-paged-live-results-20261025.md` 与机器收据。公开两页 plan，一次 Earlier，reconnect、逐段 prefix/error 与父草稿保持。 | `b0fa6e5..5b` 差异触及 paged demo/source/script，但未触及 Native endpoint crate；可复用协议与 page-plan 见证，不据此声称 5b presentation 全路径已运行。需要最后一次 5b 受影响 UI-offscreen/source integration检查，不重跑 Native 运行时分页脚本，除非该检查暴露来源错误。阅读锚点仍 `not_run`。 |
| 旧 91129d7 1026 格矩阵 | 39 passed / 3 failed / 342 blocked / 413 not_run / 229 unsupported，明确绑定旧源码；恢复通过格有独立 receipt。 | 只作证据目录与反例索引，不将格数/通过转授 5b。 |

## 结论与需补验收

1. #55 当前不能关闭：Native busy queue 是真实产品/上游阻塞；Pi direct/queued mixed、DSH busy queue 的实现路径在候选源上变化，应以冻结 5b 输入做各一次定向真实 Run workflow。其余未受影响的 runtime close/reconnect 与旧 receipt 可按源文件影响面复用。
2. #56 可复用 5b Native bounded-history + passive Summary 的窄证据和三端 recovery 运行；还需冻结 5b 下三端 locator/history/family 的差异影响核查。Pi/DSH 的真实历史来源与实际 Family 数据回传只在旧证据中，候选 UI/Presentation 变化范围须由主会话决定一次定向受影响验证；不得宣称三端 Family 已完成，直到该切片确有实际 producer 回传收据。
3. 失败/阻塞不是 Unsupported。明确契约支持的失败保持 failed；仅如 DSH 临时 fork/discard 的正式 capability 声明无支持时，才可把那一子能力列 Unsupported，并保留 `session-endpoints.md` 的固定版本依据。
4. 所有新候选程序须将源码/lock、程序 SHA、runtime identity、原始协议帧、provider 交互及 owned process exit 绑定到同一收据。源码或运行时变化后只重跑被其文件/接口影响的行；不扩成对最新 ClientKit WIP、#60–#66、桌面/IME/AX 的追逐。
