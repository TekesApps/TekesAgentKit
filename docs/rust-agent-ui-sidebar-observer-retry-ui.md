# Sidebar observer Retry UI 设计（#22）

## 目标与边界

在现有 Sidebar inventory observer 生命周期上提供用户可触发的恢复入口。恢复只重新订阅原 `SharedAgent.follow(state)`；不重连 harness、不重试 mutation、不改 Kernel、TekesUI 或 ClientKit。单端和固定多端集合都保留各自原 reader、endpoint 与 observer generation。TekesUI 仍是精确 pin 的只读依赖；业务状态组件使用其默认 Text、Button、Stack 与现有布局/样式 token，不自绘控件风格。

本轮只读对照发现，ClientKit/Tekes 当前没有 inventory observer 专用的用户重启入口。这是 AgentKit 新增的 observer-only 交互，不能宣称已有 ClientKit 行为完全对齐。可参考 ClientKit `Sources/Composer/Agent/ClientAgentSidebarListView.swift:210` 的搜索失败 Retry 呈现，以及 `ClientAgentSidebarModel.swift:249` 的查询 retry generation；实际动作仍不同。`SessionEndpointRuntime.swift:453` 的 transport reconnect 和 `:1585` 的 mutation 后 read-side reconciliation recovery 也不等价于此入口。上述均为静态源码证据，没有运行参考宿主。

## 来源与授权

重试资格必须来自“展示的原始发布”和它对应的私有来源证明。绑定层提供一个不可拆分的 observation snapshot/packet：包含原 `SidebarPublication`（多端时包含原聚合发布及每端 status）和由同一 reader 状态取得的 opaque retry key。两者在同一锁定快照或同一不可变内部对象中配对，宿主桥接只转发该 packet；不能分别读取 publication 与 generation 后再拼接。

opaque key 仅能由绑定层从真实 binding owner、endpoint 和该次发布的 observer generation 创建。它不序列化、不公开 owner Arc，也不接受调用方传入 generation 后由绑定层附上私有 owner。`SidebarPublication` 当前是可克隆、字段可变的普通值；因此不能提供 `key_for(publication)` 这类 API，让宿主编辑或替换 generation 后取得看似可信的 key。用于 restore fixture 的投影也不能改变 observation 身份。多端 facade 的默认 observation 不是任何 child reader 的授权来源。

组件只展示 packet 中的 observation 状态，并在用户按下 Retry 时发出带原 key 的 typed intent。宿主将 intent 交给持有原 binding 的协调层：先核验 key 的 binding owner、endpoint、展示代数与当前失败代数一致，再从该原 binding 捕获现有一次性 `SidebarObserverRetry` token，最后调用既有 `restart_observer(token)`。多端路径还要核验固定 aggregate owner，并只路由到 key 标明的 child binding。不能在点击时从“当前最新 publication”重新生成 key，否则旧按钮可能重试后来发生的新失败。已有 restart token 的非 Clone、跨 owner、跨 endpoint、旧 generation 和重复使用拒绝语义继续作为最终防线。

若 presentation 层提供跨 crate 的键工厂，必须要求传入 binding 私有原 owner Arc；公开可克隆的展示 source key 不能作为派生任意 generation 键的能力。外部自建 owner 产生的键必须在原 binding 被拒绝。

## 展示与控件

新增 AgentKit 业务组件 `SidebarObserverStatus`，采用 TekesUI 默认基础控件。单端 `Failed` 展示诊断与 Retry；`Restarting` 展示恢复中的状态并保留原错误/陈旧标记；收到新代合法状态帧后才转回 `Observing`。restart 返回 `Ok` 仅表示接纳，不代表恢复完成。`Observing` 状态不展示 Retry。

`Sidebar` 在现有 search footer 下方增加 observer footer。Search 保持当前 `body.children[1]` / `root + 8` 布局与 ID 规则；observer 使用 `body.children[2]` / `root + 9`，并参与 Sidebar 总高度测量。小高度时两个 footer 都应可达或按既有滚动/裁切契约明确处理；不得让 observer footer 覆盖 search 控件。动态 observer action ID 使用 `[root + 0xFE000000, root + 0xFF000000)`，search 继续使用 `[root + 0xFF000000, root + u32::MAX)`；普通控件上界相应低于 observer 区间。ID 高水位跨 Single/Multi 切换及来源替换单调前进，已退休的 Retry ID 永不重新分配。

## 状态更新与并发

组件提供 `replace_source`、`update_from`、`take_intents` 等与现有 search status 一致的窄接口。来源 key 至少区分 Sidebar host source、单端 binding 或固定 aggregate，以及 endpoint；状态修订还携带该次 observer generation。更新必须来自新 coherent packet。相同 key 的 pending gesture 在无关 endpoint 更新中保留；A 端 pending 不因 B 端刷新清除；来源替换、generation 前进或控件退休会使旧 raw action 失效。重复点击同一失败代最多采纳一次；点击完成后只有新代合法帧才能清除恢复状态。

多端组件逐 endpoint 展示状态：A 失败可单独 Retry，B 的健康状态及行继续可用。聚合视图保留完整 child publication/key 配对，不从扁平 facade 的默认单端字段推导重试资格。Single/Multi 切换建立新来源代并退休旧 ID；相同 endpoint 字符串不代表相同 binding owner。

## 计划改动文件

- `crates/presentation/src/sidebar_observer.rs`（新增）与 `crates/presentation/src/lib.rs`：只读展示 DTO、opaque source/retry key 与 typed intent；键字段私有、不可序列化，generation 由原 reader packet 产生。
- `ui/bindings/src/sidebar.rs`、`ui/bindings/src/sidebar_multi.rs`：单端/多端 coherent observation packet；按展示 key 捕获 retry 的校验入口；复用现有 retry/restart token 与 owner 检查。
- `ui/demo/src/sidebar_host.rs`：把原 binding 的 publication 与 key 作为同一个只读 packet 送到宿主；Single/Multi 路由保留原 child 来源，不从 facade 伪造来源。
- `ui/components/src/sidebar_observer_status.rs`（新增）、`ui/components/src/lib.rs`、`ui/components/src/sidebar.rs`：默认控件状态子组件、typed intents、并排 search/observer footer、动态 ID 区间和高度布局。
- `ui/demo/src/component_demo.rs`、`ui/demo/src/native.rs`：消费 observer packet，处理 Retry intent 并把结果状态继续从原 binding 投影回来；不新增恢复状态机。
- `ui/consumer/tests/`、`scripts/check-public-agent-ui-consumer.py`：workspace consumer 做来源隔离覆盖，独立外部 package 实际编译并运行其公开构造/门禁路径；不把编译当作有效恢复 IO 验收。
- `docs/rust-agent-ui-sidebar-observer-recovery-20261024.md`、`docs/rust-agent-ui-issue-plan.md`：实现后回填 #22 链接与验证范围；不以 UI 接线完成为由关闭仍含桌面验收的宽 Issue。

若现有 crate 拓扑或 Sidebar bridge 在实施前变化，按当前 workspace member 调整实际落点；不要为此引入 TekesUI API。TekesUI 若缺少现有默认控件所需的语义或布局能力，单独形成 TekesUI issue 后等待其 API。

## 验收

1. 单端旧 packet/key、相同 endpoint 的替换 binding、旧 generation、错误 endpoint、伪造/改写 publication、重复点击均不能重试；当前 Failed packet 的 key 只采纳一次。
2. Multi A/B 同 session ID：A 重试只退休 A 的旧 action/read receipt，B 保持；B 更新不取消 A pending；A 的旧 key 不会重试新代或新 owner。
3. Restarting 保留原行、活动锚点与诊断并标 stale；仅新代合法帧结束 Restarting。首次失败的 loaded/converged 语义保持现状。
4. Search 与 observer footer 同时存在、同一视口下尺寸计算稳定、动态 action ID 无交叠且永不复用；来源更新及 Single/Multi 往返覆盖。
5. UI workspace tests、严格 all-target Clippy、demo build、独立 public consumer 和来源边界检查通过；为本次 head 查 PR #10 CI。自有离屏 renderer 可补状态截图，但不得称为桌面验收。

桌面窗口、Screen Recording、物理中文 IME、AX、真实 Native/Pi/DSH 断流与窗口尺寸验收仍未验证，也不属于本设计阶段。Issue #22 的分页 continuation、目标行像素可见性和 TekesUI #53 nested detail 失败仍各自保持开放。
