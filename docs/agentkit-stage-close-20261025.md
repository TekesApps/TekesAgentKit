# AgentKit 阶段交付冻结：2026-10-25

关联 Epic #14；协议 #55/#56、集成 #58、发布 #59。暂停扩展功能，最新执行状态唯一入口为 [Epic #14](https://github.com/TekesApps/TekesAgentKit/issues/14) 正文；本文保存冻结范围与批次证据。冻结不表示适用验收已通过，也不缩减完整 Epic。

## 已验证批次摘要（2026-10-26）

本节封存813f851批次事实；最新PR head、工具版本、CI和下一步只在Epic #14正文维护。下文不可变输入及历史记录保留原身份，不从历史评论推导最新状态。

- **产品**：冻结范围5b；当前产品源码 `10263722b5fb128922c49a31b5942c41333cd6e2`，只含本范围阻断修复；ClientKit参照c289，不追新行为。原9 WIP逐文件SHA未变。
- **PR/已交付**：PR10最近已完成CI的head `02e0bab96c200fd4d10b9c52798ba00c7bc398c9`；headless、UI常规及独立consumer通过，必需nested reading失败，demo build跳过，无formal review，仍draft。main `1dcc521` 已交付独立Pi窗口恢复修复#68（PR69/main CI40227909675通过）；本批没有新入main能力。
- **验收工具**：ordinary observer e978f5e/两角色固定程序及完整绑定保持冻结；Native Family三入口程序和源码闭包见[native-family-observer-programs.json](evidence-bindings-20261026/native-family-observer-programs.json)。它们已完成定向构建/strict Clippy，Family正式样本仍0；candidate20已真实跑到完整协议/清理，离线来源对账通过；pilot仍因收尾pilot标志和RSS注册前角色竞争失败。原收据保留，先离线修复/消除采样歧义，不能开始原21-case矩阵。
- **已知有效性能失败**：Native lifecycle快照1.661691倍、首助手1.360828倍；Pi warm快照1.341904/1.313128倍、life Earlier20 1.265616倍；DSH warm首助手1.262329倍、life cleanup OS保守上界1.273253倍（各适用相对预算1.2）。原样本保留，无豁免，不用其他组通过抵消。
- **上游阻塞**：TekesUI53阻PR10必需nested reading；Kernel86阻Native受支持busy queue（不能改Unsupported）。两者原公开版本尚未证明修复。本库不改上游。
- **尚缺**：Native Family端到端pilot及原矩阵；三端相关附件/队列性能与未完成协议子项；有效性能失败的有界归因与必要产品修复；#58适用review/CI/consumer/main；#59阶段发布。桌面/IME/AX均not_run；#60–66/#70后续，不加入当前前置。
- **顺序/批次结束条件**：[有限离线归因](agentkit-stage-performance-attribution-20261026.md)已结束：当前缺操作关联分段，没有证据支持本库修复，原失败仍阻塞；并行只读审查不改冻结输入。然后一个Native Family pilot须raw、原始页、OS身份、退出、最终报告全链路一致且仍diagnostic；再按原矩阵采正式数据。复用既有闭包审查：#68已独立交付；#67依赖main尚无的DSH queue/projection闭包，不能只摘两文件形成独立交付。本批不重新评估所有拆分。仅输入/上游变化才重判；条件齐备即集成，未齐写具体依赖。全部适用门禁满足才发布，不预告时间或百分比。

## 不可变输入

- AgentKit范围基线 `5b7c84361206b45ac381b10437097d3b76c322c1`；阻断修复 #67 为 `4274fe82fc353ff06100d4106d527e25dc5dfca3`，#68 为 `98b1d9a2960b90774ea5454e7e8dad870538f7bc`（分别取得精确程序窄验收）；当前产品源码 `10263722b5fb128922c49a31b5942c41333cd6e2` 已吸收 main 的独立 #68 修复与增强回归；本地保护引用 `refs/agentkit/stage-close-20261025/base`。PR #10 为 draft。
- 当前 main `1dcc52153ea45ba6ce73a135a8d8fe8d3406360c`：独立两文件 PR #69 已合入，main CI `40227909675` 通过，#68 已关闭。冻结对照/性能 baseline 仍为原 main `f1daa0ffcf6ced41e21f351e22845455ca78006c`，不随集成漂移。已发布 tag 仍为 headless v0.3.8；本阶段未发布。
- ClientKit 行为参照 `c289bc9d33edcd83a782ac17927a8c65a027eccd`；Tekes `8858ccd9d46590852ec7a0949aee289fc6798d70`。此前记录的未提交参照候选仅按原冻结快照解释，不能当已发布 SDK。
- TekesUI 公开精确 pin `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；8 包、243 输入闭包。公开 main `7c13ec0` 未改变该闭包，不等同 #53 修复已发布。
- Native Kernel 0.9.3：运行程序对应 `09508a9083c24c7accefc99516739f279b85913e`，不是兄弟库现 HEAD。Pi 1.3.0 / Node 24.20.0；DSH 0.2.1-rc.2。程序摘要见[协议证据审计](agentkit-stage-protocol-evidence-audit-20261025.md)。本期不升级运行时以追逐开发分支。
- 后续仅允许本范围阻断修复；修复产生新 head 时记录受影响输入和定向回归，不能自动转授旧通过。

## 本阶段必须交付和验收

1. 冻结源码中已实现的 SEP/presentation：三端发送、取消、流式、附件/队列、pointer/公开历史/分页/重连，以及各自合法公开 producer 的家族数据回传。#55/#56 各自拥有真实协议与非桌面性能验收；静态测试不代替实际协议。[有限矩阵](agentkit-stage-protocol-matrix-20261025.md)及[性能门槛](agentkit-stage-performance-profile-20261025.json)独立冻结；ordinary正式来源协议已闭环，性能保留上述有效失败，Family/附件/队列性能尚未完成。
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
| #58 集成 | #68 独立两文件闭包通过精确 head review、CI、公开 consumer、固定 Pi 协议；PR69 已合 main1dcc521，main CI40227909675成功。PR10精确02e0bab root/独立consumer通过，strict nested reading失败 | PR10保留严格 nested reading 和协议范围门禁；其余票不因69合入而关闭 |
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

### 2026-10-26：普通正式组收口与来源诊断

[三端正式绑定](evidence-bindings-20261026/ordinary-official-cohort.json)记录42计划、40实际执行：38通过原单case契约，DSH两角色lifecycle在原summary整值oracle失败，Pi两角色lifecycle仍not_run。三端cold和Native warm满足原预算；Native lifecycle scope0恢复快照1.661691倍、scope1首助手1.360828倍，Pi warm快照scope0/1分别1.341904/1.313128倍，DSH warm首助手scope1为1.262329倍，均超过1.2。完整组即使其他组缺失也必须报告这些有效失败，不能用未完成矩阵遮蔽，原收据未替换。

[DSH来源诊断](evidence-bindings-20261026/dsh-inventory-source-diagnostic.json)两scope各204条完整历史不变，但scope1的live turnOutline在立刻抓取时仍落后（466，response为空），重连后469补齐；只有scope0全部live value不变，不能扩报到两scope。缓存summary不是fresh SDK来源。新[来源契约](agentkit-reconnect-source-convergence-contract-20261026.md)要求独立fresh follow、SDK明定的exact event cursor、完整值与内容、水位和Pi稳定文件metadata证明；目前是尚待实现与适用验证的设计，不把旧失败改成成功。

[Native线路诊断](evidence-bindings-20261026/native-wire-delay-diagnostic.json)单次冷启动在原样转发的loopback proxy记录ack→provider约154/182ms，provider fixture自身约1.66/0.31ms；50ms OS采样在两者之间首次见worker。只读源码未找到AgentKit固定轮询，Kernel路径有lazy worker/context/lease，尚不能归因特定缺陷；代理与采样不替代正式样本，不豁免相对延迟失败。

PR10精确72a0d64 CI40233798349/40233792652终态为headless成功、UI失败；独立consumer通过，必需strict nested reading仍accepted=false（parentFollowingAfterInnerUp=true，append offset634，detailVisibleAfterAppend=false），demo build跳过，artifact12613329961保留。PR10仍draft，#55/#56/#58/#59未完成，不新增发布。全量Python验收工具检查481项通过（python312-cohort-diagnostics-final-v1.log）；本检查点没有Cargo或桌面/物理IME/AX执行，原9 WIP保持。

### 来源收敛补充组：保留新结果与新失败

共同observer e01a23b已按精确baseline f1/candidate102分别完成8项Rust定向检查、strict Clippy和构建，程序10c630a4/34c0707b。4case在执行前登记（plan SHA8608f830…），固定运行时/原预算，未重跑Native/warm，原40case保留；新组不混算旧observer时间。

[补充正式绑定](evidence-bindings-20261026/reconnect-source-official-cohort.json)：Pi两角色初始化+10测量轮全部公开pointer/完整native entries、新generation、owned sessionFile稳定metadata与恢复summary时间来源通过。材料器最初错把reconnect缺少冗余iteration字段判为失败；现仅离线按原iteration_started区段关联，同份原始events/sidecar完整对账，原error保留，协议重跑0。Pi来源恢复通过不等于性能全过：scope0 Earlier20 p95 20.52075/16.214041ms=1.265616倍，超原1.2，其他该组逐scope延迟/绝对/RSS按原报告解释。

DSH两角色初始化reconnect成功，首个测量轮before fresh witness等待runtime追齐超时；不改Unsupported、不换样本、不认定是产品/SDK缺陷。原事件只保存fresh witness，缺超时当刻每项runtime predicate状态，因此尚不能把失败归到keys/value/history/control_ready中的某项，也需核对before等待是否超出恢复契约。完整组尚未验收，等待只读归因，不无限扩大矩阵。

本检查点四case各一次，原40与新4分开保存（累计44次物理正式执行，不把旧Pi两个not_run改写）；所有原有效相对失败保留。未完成Family/附件/队列性能、TekesUI53/Kernel86、review/CI/桌面/集成/发布仍各自保留。无产品代码变化、无Kernel/Swift/SDK修改、无桌面；原9WIP逐文件SHA保持，构建复用唯一SSD target。

### 三端普通协议分组已补齐；性能仍未通过

DSH纠正observer e978f5e、baseline950db9a1/candidate92206559的新两case各执行一次，初始化+10测量轮全部来源契约通过：独立完整公开source200确实耗尽；两session原保留floor/完整entries内容与顺序、旧projection水位不回退、恢复后全部值和summary/time来源、新generation均对账，owned进程退休通过。原缓存窗口与rolling50不能整值相等的错误已纠正，既保留未加载前缀，又不丢任何旧行；旧失败没有删除或改写。详见[DSH v2正式绑定](evidence-bindings-20261026/reconnect-source-official-cohort-v2.json)。

普通场景现在有各自精确程序的42个适用成功分组case证据：Native原14、Pi原cold/warm12+新life2、DSH原cold/warm12+新v2 life2。共46次物理正式执行（原40+v1四+v2二），额外四次原DSH错误oracle失败继续保留；不是把46都称成功，也不将不同observer时间混算或统一转授新head。

性能仍未通过，保留全部有效失败：Native life恢复1.661691倍/首助手1.360828倍，Pi warm快照1.341904/1.313128倍，Pi新life Earlier20 scope0 1.265616倍，DSH warm首助手1.262329倍；DSH新life cleanup保守OS退休上界1060.203834/832.673125ms=1.273253倍，超原1.2（10s绝对门槛通过）。没有调整预算或挑选样本。

#55/#56完整Family/附件/队列性能及相关原矩阵未因ordinary成功而关闭。TekesUI53和Kernel86最新只读核对仍OPEN；PR10仍draft、#58/#59未完成，main仍1dcc521且仅#68独立切片已合入。原9 WIP保持，无Kernel/Swift/SDK修改、无桌面/物理IME/AX；后续冻结该普通组，不继续扩范围或重跑成功组。

### Native Family pilot补证与66cc CI终态

v3仅用原始数据独立离线重验完整通过，派生报告official=false/diagnosticOnly=true/performanceAccepted=false，原failed/identityRechecked=false收据不改写。root角色修正的唯一32KiB样本有完整PID/start/executable/父与时间边界、退出证明；RSS保守峰值未减。原/纠正后/保守峰值47568/20064/88208KiB，最大gap56.535708ms。正式计划仍原21case：首个baseline/common50/cold0采集成功，材料器只修公开event.time:f64的整数/浮点表示比较后，同份raw离线通过；未开始20个登记仅更换post-capture reporter，runner/provider/sampler/observer/binary不变，不重跑first，不增加样本。v1/v2登记和负报告均保留。

精确66cc66c的CI40247962897/40247958595结束：headless、UI fmt/Clippy/tests、child source witness、Native审批consumer编译和standalone public consumer均成功。唯一失败仍mandatory strict nested reading（parentFollowingAfterInnerUp=true、append offset634、detailVisibleAfterAppend=false），demo build skipped，artifact12619153245与terminal-binding保存。无新UI产品变化，不弱化门禁、不合入/发布。

### Native Family原21-case正式矩阵采齐

[正式绑定](evidence-bindings-20261026/native-family-official-cohort.json)记录21个原登记case各采集一次：所有适用source/ChildRun50、独立SEP20、真实ChildRun20、Earlier、旧实例重连与进程退休通过。common50 cold/warm/life全部原绝对/相对预算通过，candidate20全部原绝对预算通过；不把没有同配置入口baseline扩成basic Family豁免。两次post-capture材料器误判只离线复用原raw纠正，不替换采集、不混observer；负报告/旧登记与程序仍归原身份。当前只完成Native Family性能子范围，ordinary有效失败及Pi/DSH Family、附件/队列继续未完成，不合并、不发布、不关闭整票。最终510项Python检查通过，无Cargo/桌面/IME/AX。
