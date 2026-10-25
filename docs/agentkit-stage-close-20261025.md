# AgentKit 阶段交付冻结：2026-10-25

关联 Epic #14；协议 #55/#56、集成 #58、发布 #59。暂停扩展功能，本文是当前执行入口。冻结不表示适用验收已通过，也不缩减完整 Epic。

## 不可变输入

- AgentKit范围基线 `5b7c84361206b45ac381b10437097d3b76c322c1`；唯一阻断修复候选 `4274fe82fc353ff06100d4106d527e25dc5dfca3`（#67）；本地保护引用 `refs/agentkit/stage-close-20261025/base`。PR #10 为 draft。
- main `f1daa0ffcf6ced41e21f351e22845455ca78006c`，已发布 headless v0.3.8。
- ClientKit 行为参照 `c289bc9d33edcd83a782ac17927a8c65a027eccd`；Tekes `8858ccd9d46590852ec7a0949aee289fc6798d70`。此前记录的未提交参照候选仅按原冻结快照解释，不能当已发布 SDK。
- TekesUI 公开精确 pin `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；8 包、243 输入闭包。公开 main `7c13ec0` 未改变该闭包，不等同 #53 修复已发布。
- Native Kernel 0.9.3：运行程序对应 `09508a9083c24c7accefc99516739f279b85913e`，不是兄弟库现 HEAD。Pi 1.3.0 / Node 24.20.0；DSH 0.2.1-rc.2。程序摘要见[协议证据审计](agentkit-stage-protocol-evidence-audit-20261025.md)。本期不升级运行时以追逐开发分支。
- 后续仅允许本范围阻断修复；修复产生新 head 时记录受影响输入和定向回归，不能自动转授旧通过。

## 本阶段必须交付和验收

1. 冻结源码中已实现的 SEP/presentation：三端发送、取消、流式、附件/队列、pointer/公开历史/分页/重连，以及各自合法公开 producer 的家族数据回传。#55/#56 各自拥有真实协议与非桌面性能验收；静态测试不代替实际协议。[有限矩阵](agentkit-stage-protocol-matrix-20261025.md)及[性能门槛](agentkit-stage-performance-profile-20261025.json)独立冻结；性能门槛尚无实测通过。
2. 审查已验证待集成能力的实现、依赖和证据，不继续添加新业务组件。#58 负责精确 head review、适用 CI、独立公开 API consumer、main CI。
3. #59 仅在范围内门禁通过并进入 main 后发布：稳定 headless 与实验 UI 明确区分。发生公开 API 源码不兼容时评估 minor 版本，不能只凭 serde 兼容发 patch。

本阶段交付候选不承诺完整 UI parity。PR #10 原 UI 门禁仍必需；若拆分，必须按代码闭包真实隔离并保留各自标准，不能用拆分免除仍适用的真实协议失败。

## 已验证待集成与剩余

| 范围 | 当前事实 | 下一步/关闭门槛 |
| --- | --- | --- |
| #12/#13/#29、#33/#34/#38/#41–#44 | 原版本证据已归档；仍 OPEN、未进入 main | 按各票标准审查，确认本次包含的代码/适用验证；不能根 workspace 合入就关闭 UI 能力 |
| #35/#36/#37/#40/#45 | 冻结提交包含部分实现，不证明整票完成 | 仅收口已有实现和受影响验收；新差异后续处理 |
| #55 Run | 5b headless CI通过；旧 Pi/DSH 通过绑定旧程序 | Pi/DSH mixed队列/历史/重连，以及Pi取消/Unknown原ticket自动对账窄通过；其余端取消、混合工具推理/性能仍有缺口；Native queue保留Kernel86真实失败 |
| #56 session/family | Native 两页子历史/passive Summary及 v5 原父 Task 审批拒绝失败通过；Pi primary child running→failed/history/reconnect窄证据通过 | DSH primary-child正常/失败来源及历史重连也已窄通过；SidePanel child pane、适用剩余父草稿/源退休与性能仍有缺口；不扩大原收据范围 |
| #58 集成 | cfecb95 PR CI 40207891571：root与UI常规检查、child witness、新Native approval consumer、独立公开consumer通过；严格nested detail reading失败，后续demo build跳过；无formal approval | 保留TekesUI #53必需门禁和协议范围阻塞；不能据局部通过合并/发布 |
| #59 发布 | 待办，尚无本阶段新 tag | 精确集成 head、main CI 与范围文档通过后发布 |

## 真实阻塞、等待和后续范围

- TekesUI #53：当前 PR #10 必需的 nested reading 检查失败；保留失败，等待适用公开修复。#39 窄宽 footnote labelVisible=false 等原 UI 负证据不改写为通过。
- Kernel #86：Native busy input 已 admission 而公开 queue/control 为空，是契约支持能力的真实失败；不是 Unsupported。本库不改 Kernel；最新公开 release 仍 0.9.3。它阻塞适用 #55 队列验收和涵盖该能力的阶段承诺。
- #47–#54 仅等待各自对应的公开基础能力/producer，不阻无关子项。
- #57 保持等待桌面；桌面、物理 IME、AX、截图/TCC 均 not_run。本阶段不降低它原来的最终标准。
- #36 新跨格式恢复、#40 新悬停预览、#46 未注册宿主适配为冻结 WIP，非已验证交付。保留源码与 `.runtime-fixtures/stage-close-20261025/wip-entry/manifest.json`、patch 和逐文件快照；不为了清理阶段而丢弃。
- #60–#66 是后续 P2 待办：工具 Tabs、Files、Changes/Diff、Terminal、文件预览、Browser、Composer Palette。已同步需求 R44–R50；无新实现/验收声明，不自动加入本阶段前置，完整 Epic 仍要求完成。

## 构建与证据规则

主会话串行 Cargo，先核对进程、锁、后续依赖。所有验证使用冻结源；当前主目录含未提交 WIP，不在其上开始批次。复用 SSD `/Volumes/HP P900/dev/TekesAgentKit/shared-target`，不新建 target、不删未知缓存。构建期间任何会改变批次输入的编辑须延后；文档与源码输入界限记录于 manifest。

定向检查通过后只运行适用最终检查；没有新变化/失败不反复全量重跑。原始输入、程序 SHA、运行时握手、公开协议帧、provider 请求、进程退出与实际 not_run 同一收据绑定。历史 91129d7 的 1026 格以及 1d/29ea/06c/5b 各收据各守身份，不相互扩报。

当前不合并、不打 tag，不是撤销授权，而是尚未满足适用门禁；已授权集成在精确检查完成后直接执行。不得降低检查、关闭未合入代码票或宣称完整对齐。

## 封存结果

本轮追加Pi真实子代理Summary/source及历史重连窄证据；DSH Summary来源脚本在固定60秒readiness期限失败，尚未进入投影，负证据和进程退出保留。它是独立剩余验收，不由DSH mixed队列成功替代。详见有限矩阵和结果JSON。当前仍不合并/发布；不以文档冻结宣称本阶段已交付。

### Subsequent narrow checks

PR head `30494699f199259abfddfe4621a4e828d620f415` CI run `40157167303` completed: root checks and UI/public-consumer checks passed through the standalone consumer; the mandatory strict nested detail reading step failed on the published TekesUI input. The later demo build step was skipped, so this run does not prove it. No merge/release gate was waived.

Pi primary child running→failed/history/reconnect leaf evidence is now available, with the combined DSH run still failed at readiness. Native approval-denial parent failure validation subsequently passed as an opt-in offscreen public consumer (v5 receipt below). The matrix and result JSON retain separate negative evidence and unrun performance scope. Source remains `5b7c843`; these additions validate existing behavior and do not expand product scope.


### Native approval-denied parent Task (bounded acceptance)

`native-parent-approval-v5/acceptance.json` binds Kernel 0.9.3 (`09508a9`) to
consumer SHA `e742cafae5ba3715ba3c3be16be8a3c1f849e1a34baff8fa4d9cbfbe638d5caf`,
built over frozen product source `5b7c843` with the new opt-in acceptance example.
The original parent journal call/result proves `doing → denied/error` in the same
Run/epoch, mounted Summary view/row/title identity and unchanged nonempty parent
draft. Public catalog contains only the parent. Two loopback provider requests
include the original task and its exact continuation/result; consumer and owned
Kernel exit 0. No child failure substitutes for the parent result. Server
transactional freshness is unproven; full #56, performance and desktop remain
unaccepted. Failed prior attempts are retained. v4's provider metadata overwrote
the follow-up marker-presence flag; v5 corrects the recorder and is authoritative.
This changes acceptance tooling only, not frozen product behavior or gate scope.

### cfecb95 CI terminal evidence

CI `40207891571` completed with headless success and UI failure. The new Native
approval consumer and standalone public API consumer both passed. Downloaded
artifact `12604852934` records strict nested reading `accepted=false`,
`parentFollowingAfterInnerUp=true`, parent append offset `0→634`, and
`detailVisibleAfterAppend=false`. This confirms the existing TekesUI #53 reading
intent failure; the subsequent demo build was skipped. Local evidence is retained
under `stage-close-20261025/ci-cfecb95-ui-inputs/`. No gate was disabled or weakened.

### 2026-10-26 blocking fix #67

DSH family reached the original completed child Summary, then real reconnect
failed with `unknown DSH projection authority`. Fixed public DSH distinguishes
cached listing hints from live sequenced cuts. #67 is the one added implementation
bug ticket under existing requirements; it is not a new feature or test ticket.
Its patch is under targeted compile/protocol verification. Native cancellation
has separately passed with original journal identity and normal owned exits;
older failed fixture attempts remain preserved. Do not promote these old program
receipts to a subsequent #67 binary without an explicit affected-input review.

### Candidate4274 verified, phase not released

The only authorized product fix after scope base5b is #67's DSH cached-list
boundary. Its exact candidate4274 / SHA081c3fe passed the original DSH completed
Summary/reconnect plus DSH-only running→failed child Transcript/history/reconnect.
Raw cached hints are retained without sequenced authority. #67 is OPEN and
verified-pending-integration. Earlier Pi/Native proofs retain their original
source/program identities; no wholesale promotion to4274 is implied.

CI4274 root and standalone consumer passed; strict reading still fails on the
published pin. TekesUI53 and Kernel86 remain open when checked2026-10-26. #55/#56
still need remaining source/host coverage and preregistered performance; no
merge/tag/main CI/Issue closure occurred. #60–#66 remain future scope.
