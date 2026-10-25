# AgentKit 阶段交付冻结：2026-10-25

关联 Epic #14；协议 #55/#56、集成 #58、发布 #59。暂停扩展功能，本文是当前执行入口。冻结不表示适用验收已通过，也不缩减完整 Epic。

## 不可变输入

- AgentKit范围基线 `5b7c84361206b45ac381b10437097d3b76c322c1`；阻断修复 #67 为 `4274fe82fc353ff06100d4106d527e25dc5dfca3`，#68 为 `98b1d9a2960b90774ea5454e7e8dad870538f7bc`（分别取得精确程序窄验收）；当前分支 `10263722b5fb128922c49a31b5942c41333cd6e2` 已吸收 main 的独立 #68 修复与增强回归；本地保护引用 `refs/agentkit/stage-close-20261025/base`。PR #10 为 draft。
- 当前 main `1dcc52153ea45ba6ce73a135a8d8fe8d3406360c`：独立两文件 PR #69 已合入，main CI `40227909675` 通过，#68 已关闭。冻结对照/性能 baseline 仍为原 main `f1daa0ffcf6ced41e21f351e22845455ca78006c`，不随集成漂移。已发布 tag 仍为 headless v0.3.8；本阶段未发布。
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
| #55 Run | 5b headless CI通过；旧 Pi/DSH 通过绑定旧程序 | Pi/DSH mixed队列/历史/重连，以及Pi取消/Unknown原ticket自动对账窄通过；Native/DSH原turn取消亦按独立程序窄通过；混合工具推理及同次usage来源已取得窄证据；正式性能未验收，Native queue保留Kernel86真实失败 |
| #56 session/family | Native 两页子历史/passive Summary及 v5 原父 Task 审批拒绝失败通过；Pi primary child running→failed/history/reconnect窄证据通过 | DSH primary-child正常/失败来源及历史重连也已窄通过；Pi/DSH真实SidePanel child pane分页/重连、父草稿与退休窄证据已补齐；完整适用Family及性能仍未验收；不扩大原收据范围 |
| #58 集成 | #68 独立两文件闭包通过精确 head review、CI、公开 consumer、固定 Pi 协议；PR69 已合 main1dcc521，main CI40227909675成功。PR10精确98b root通过，UI失败 | PR10保留严格 nested reading 和协议范围门禁；其余票不因69合入而关闭 |
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

PR #69 已按授权独立合入；PR #10 与阶段 tag 尚未满足各自适用门禁。后续独立闭包仍在精确检查完成后按授权集成。不得降低检查、关闭未合入代码票或宣称完整对齐。

## 历史封存结果（5b原程序，不覆盖后续窄证据）

本轮追加Pi真实子代理Summary/source及历史重连窄证据；DSH Summary来源脚本在固定60秒readiness期限失败，尚未进入投影，负证据和进程退出保留。当时它是独立剩余验收，不由DSH mixed队列成功替代；后续4274的正常/失败Summary与重连窄证据按下文独立绑定。详见有限矩阵和结果JSON。当前仍不合并/发布；不以文档冻结宣称本阶段已交付。

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
Its patch subsequently passed targeted compile/protocol verification at4274; the receipts below retain exact identities. Native cancellation
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

### 2026-10-26 original mixed tools and independent child panes

#55 now has fixed Native/Pi/DSH original offscreen component proofs for three
actual read tools, reasoning, continuous grouping and retained controls,
synthetic preedit/focus and draft isolation, and reconnect de-duplication.
Product4274/headless081c3fe/UI7d269e26 retain distinct identities. Usage-specific
source acceptance and preregistered performance remain open.

#56 now has actual mounted Pi/DSH child pane Earlier and host reconnect proofs,
plus the original controlled running→failed source pair, nonempty parent draft,
selection isolation, and close/retired-source fencing. Pi UI6aa75731 records
50→62→62 source rows; DSH UId63dea95 records50→97→97, epoch2→5 for both.
All original record content/order is preserved without duplicates. Additional
original DSH request/prompt context rows may interleave after raw journal paging;
old messages need not be a contiguous suffix. The stricter suffix assertion's
failed receipt is preserved. These are acceptance-only overlays, not new product
features or desktop/IME/AX/pixel-anchor/performance acceptance. Portable scope
bindings are in `evidence-bindings-20261026/mixed-tools.json` and
`evidence-bindings-20261026/child-pane-recovery.json`.

Exact f833 CI40214966220 finished: root, ordinary UI checks, child witness,
Native approval and independent public consumer pass; strict reading fails on
TekesUI53, later demo build skipped. Artifact12607411641 retains the original
negative result. New later commits must obtain their own CI; no transfer of f833
checks is implied. PR10 stays draft; no merge, tag or code-issue closure.

### 2026-10-26 usage and current CI

Original same-publication usage now has [portable bindings](evidence-bindings-20261026/usage-source.json) over product4274/UI53f97f35: Native numeric transient20→reported final60; DSH running durable-paired20→reported final60 (no numeric transient claimed); Pi durable current-three-message output60 with contract-declared reported/live absence. The DSH initial verifier failure remains intact; stricter offline v2 raw-source reanalysis resolves the verifier error without rerunning protocol or promoting another binary. Full #55 and performance remain open.

Head70f4084 CI40216799309/40216795717 terminated: root passed, normal UI checks and standalone consumer passed, strict nested reading failed; demo build skipped. Artifact12607784167 records accepted=false, parentFollowingAfterInnerUp=true, append offset0→634 and detailVisibleAfterAppend=false. No applicable gate was removed. The split review found no currently verified dependency-isolated delivery that can remove these gates; PR10 remains draft. Main already contains stable headless v0.3.8.

### Finite performance diagnostic checkpoint

[Preflight](agentkit-stage-performance-preflight-20261026.md) and its portable
binding record the Native candidate cold public-protocol plumbing case, including
verified100-body/10-tool seed per session, real harness restart, page20, pointer
reconnect, two sends and actual RunController50, with owned exits. Failed pilots
are retained. Official samples remain0; the required paired trajectories, other
endpoints, family/attachment and all performance budgets are unaccepted. These
are acceptance tools for existing #55/#56 criteria, not new product capabilities.
The only product delta after5b remains #67.

### #68：Pi 恢复窗口真实阻断缺陷

[修复说明](agentkit-pi-window-message-units-20261026.md)和原负证据绑定已建立。恢复窗口把custom entries计成消息会扩大历史、使分页提前耗尽；#68仅修这个单位，不重置/丢弃已加载窗口。#55/#56适用Pi行保持未通过，等待精确新程序；不能由4274旧证据升级。#67是此前唯一产品修复的历史记录仍保留，当前允许修复列表增加#68。

7c94123 CI40223657233已结束：root及UI常规/独立consumer成功，唯一失败仍为strict nested reading，后续demo build跳过；artifact12610885705保留。未弱化原门禁，PR10仍draft。

### 2026-10-26：真实隔离Pi切片已进入main

更深入的源码级审查取代此前仅按大PR闭包无法拆分的初判：#68仅需main两文件私有Runtime/回归测试，不依赖UI、NativeJournal/queue或新API。PR69（精确head4c682b1）通过workspace全部检查、独立公共consumer和固定Pi真实窗口恢复，合main1dcc521；main CI40227909675成功，文件树与被验候选相同。#68已关闭，完整[证据绑定](evidence-bindings-20261026/issue68-mainline-delivery.json)保留新DUT66a与旧seeder18d8身份。

PR10已本地吸收main至1026372；冲突保留当前实现与main更强的50 close/open测试，29项WIP逐文件核对未变。其余待集成项及#55/#56验收、TekesUI53、Kernel86、#57桌面、#59发布仍保持原标准和阻塞，不据此合并PR10或打阶段tag。稳定headless的main增加这一个Pi修复，不宣称完整对齐。

### 收口工具检查点（1026372 product + 显式验收 overlay）

原9 WIP逐文件SHA保持，main69同步后的Runtime定向回归通过。冻结 `stage-main69-synced-performance-source-v1` 编译performance consumer并通过严格Clippy；程序SHA `a637ddb8fac00854b5883fe14eef850e6808c11c1d0704f390d08b0b085cd439` 已放固定candidate路径，旧程序归档，不回灌target。Python3.12脚本检查通过（日志 `python312-main69-final-v1.log`）。

验收工具补充：公开历史按原message数量与完整entry内容/顺序验证、DSH显式runtime-context排除、warm进程边界与保守cleanup时钟、配对样本报告和有限Native家族seed候选。后者尚无真实pilot通过；配对正式样本仍0，不能把工具测试或单次诊断扩报为性能达标。新Pi冷启动诊断另守其实际收据，不自动成为正式样本。所有构建复用SSD共享target；未启动桌面/物理IME/AX，未改Kernel/Swift/SDK。

本检查点的Pi cold诊断已完成：两会话各100条1024-byte正文、10个原read工具组；公开page20、RunController固定50窗口分页、重连和close/open实际走通，原内容/顺序保留，进程退休核对通过。绑定 `ordinary-diagnostic-main69.json` 明确它只有diagnostic通过、正式样本0，不代替配对性能。

#67主线切片复核：main仍使用DSH旧Remote RPC/mux实现，缺当前分支queue/projection适配层；缓存hint隔离补丁依赖该闭包。现不能仿照#68直接移植少量文件，保持待集成与原真实协议门禁；不为交付跳过相关依赖或扩大重构。

固定DSH0.2.1-rc.2的同一ordinary cold诊断随后也完成（程序a637ddb8）：公开种子/两会话各100条正文与10个工具组、page20、原50窗口分页、重连、close/open和发送终态，owned进程退休均通过。`ordinary-diagnostic-main69.json` 追加DSH独立收据摘要；它不替代DSH家族或正式配对性能。旧失败不改写，正式样本仍0。

### Native 有限family seed单次pilot

固定18d8f643 headless（product98b）和Kernel0.9.3正常完成原Task与一个bounded child。61条1024-byte commentary/61个真实read组与normal final均来自公开history，runOpen=false；公开initial max20 + 一个实际Earlier20，共两页，原始记录309+1，联合内容/顺序与完整groundtruth310条逐项相等。Kernel与三次CLI退出0，owned census无残留。不是每页20条原始record，也未为第二页增长或重跑seed。原runner把委托父provider请求计为providerRequests，字段仅覆盖父请求，已在当前工具明确命名；原收据不改写。详见native-family-seed-diagnostic.json。

本次只确认正常终止与有限两页数据形态，尚未进行cold恢复/paired latency/RSS预算；正式样本0，#56不关闭。原helper和新runner是验收工具，不是产品功能扩展；不改SDK，不启动桌面。

### 正式普通样本开始与未消除的差异

两个程序已使用同一de9b1f3b observer：baseline f1/0baab01b，candidate102/a637ddb8；profile正式样本0时明确将candidate pin从98b修正到真实102，预算/数据集不变。原登记v1在任何harness启动前因Python sys导入缺失失败，日志/注册/源码保留；修正后v2登记启动首个Native正式cold样本，原始协议成功。原报告多要求了reconnect事件重复iteration字段，实际consumer按原iteration_started区段组织；离线按原区段修正解析并保留原拒绝，无新事件/性能数据合成、无协议重跑。

修正进程测量后Pi warm初始化+10轮诊断已通过，原Node改名误分类失败保留。Pi lifecycle另有summary整值比较失败；新公开snapshot补证及离线键名修正证明两会话只有updatedAt增加、完整123entries内容和顺序不变。该文件mtime来源在5b已存在，不是#68回归；与ClientKit stored timestamp存在语义差异，已建后续#70，不扩当前产品范围。原lifecycle失败不翻案，也不通过忽略字段消除它。

PR10精确d23731d CI40230369803已结束：headless、独立consumer、child witness与Native approval编译通过，仅strict nested reading失败；artifact12613590407 accepted=false、parentFollowingAfterInnerUp=true、append后offset634、detailVisible=false，demo build跳过。原门禁不弱化，#58/#59仍未完成。

### Native普通正式组：保留性能失败

新正式14case（每角色cold5独立case、warm/init+10、lifecycle/init+10）已按预注册顺序采集，全部协议和单case契约通过。冷启动和warm组的绝对/逐scope相对/RSS门槛通过；lifecycle全部绝对/RSS通过，但scope0恢复快照p95 97.482292ms / 58.6645ms = 1.661691倍、scope1首助手内容368.007459ms / 270.429125ms = 1.360828倍，超过原1.2限制。

这是有效失败，未发现测量错误或数据量不同；同轮系统load上升仅是可能混杂因素，没有证据可以归因并豁免。保持原样本，不调预算、不替换，#55记录待有界归因的Native路径。正式证据见native-ordinary-official-v2.json；不称三端或完整performance通过，其他端、家族及附件/队列仍分别未完。

### Pi/DSH首次正式cold的离线材料修正

两端baseline cold0原正式协议执行成功，但材料器误把saveAs的s0/s1别名当真实sessionId，原source/receipt/error保留。现从原create响应取assigned ID，与原consumer scope顺序独立相等校验后补出正式row；均通过单case契约，无协议重跑、无诊断升格。见public-create-pointer-normalization.json。

后续24个尚未开始的登记只更正材料器/离线报告输入；捕获runner、采样器、provider、runtime/seed/projection helper与原v2逐文件SHA一致，Rust observer/程序/SDK/profile不变。原2个已执行数据保持身份；Pi lifecycle两个计划仍not_run，不用后续#70覆盖其失败。Native相对性能失败仍保留。Python全量472项通过后的此小修定向3项通过，不重复全量构建。
