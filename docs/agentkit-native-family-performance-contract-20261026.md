# Native Family：冻结的非 UI 性能执行契约（#56）

状态：工具、独立diagnostic pilot与原21-case Native Family正式子矩阵已验证通过；完整#56/global performance仍未完成。沿用原阶段 profile 的预算、cold5、warm初始化排除+10、lifecycle初始化排除+10，不覆盖普通组，也不抵消其有效性能失败。原阶段范围5b+#67/#68，产品比较f1daa0ffcf6ced41e21f351e22845455ca78006c / 10263722b5fb128922c49a31b5942c41333cd6e2，ClientKit c289，固定Kernel0.9.3/09508a9。只实现验收工具，不改产品库、Kernel、Swift、SDK或TekesUI，不跑桌面。

## 数据与适用性

每个独立case由真实公开父Task正常生产一个同会话bounded child；复用既有producer和已通过pilot的61段1024-byte commentary、61个真实read call/result及正常final。只读公开catalog与child history证明父/子/sourceCall、内容/顺序、runOpen=false，full200须EOF。公开初20+实际Earlier20共两页（合法source-group展开为309+1条raw记录，不切成20条raw）。原pilot仅诊断，不转为正式样本。

Native closed bounded child公开契约只读；不prompt/cancel子项，不人为增长。父发送性能仍由原ordinary组承担，不能把此边界扩成Pi/DSH子会话控制豁免。warm/lifecycle源不变，所有读取后provider计数不得增长，不能创建第二个child。正常父Task/sourceCall结果、非空父草稿与child隔离、只读availability均须核对。

## 三条路径不能替代

| 路径 | 角色/样本 | 实际入口与适用结论 |
| --- | --- | --- |
| common50产品回归 | baseline+candidate，各cold5/warm1/lifecycle1个case，共14 | 两代实际`ChildRun::open`默认50、`transcript`、`load_earlier`；basic family有baseline。snapshot/reconnect/RSS/cleanup用原绝对及相对预算；actual50 Earlier保留功能与耗时，不能称page20。 |
| common50 case内独立SEP20 | 同上两角色同样工作量 | 公开`bounded_child_history(max20)`初页+真实Earlier20，交既有`SessionProjection::apply_page`与`TranscriptSnapshot::read`；page20原1s及1.2倍预算。不是完整ChildRun20产品路径，不复制Task装饰状态机。 |
| product20真实产品路径 | 仅candidate，独立cold5/warm1/lifecycle1个case，共7 | 实际`ChildRun::open_with_page_size(20)→transcript→load_earlier→transcript`；保留原实例重连。此新增入口f1没有，但basic family仍有baseline。该特定入口沿用全部适用绝对预算，明确无同入口relative结论，不能以此豁免common50/SEP20比较。 |

21个case上限。每个warm/lifecycle case各初始化1轮+测量10轮；每cold case只1轮。common50与product20不放进同一进程测量/RSS区间，不在已经被common50预热的进程称cold20。可复用代码与合法seed机制，不复用冷进程身份。

## 正式执行前与来源

每case须从完整固定registration读取并校验程序/角色源码manifest、consumer、runner闭包、Python、Kernel/worker/provider config、profile与本契约/recipe SHA；任何case目录、运行时probe或进程创建之前完成。case目录已有即拒绝，无重试/替换。正式case在开始即official，不能把先前pilot或diagnostic升级。

同版透明公共Endpoint wrapper捕获真实`subagents.history`请求、原响应/错误、当前阶段标签、原intent/return时间和该endpoint连接代号；转发原参数与原值，不改页大小、不补数据、不复制projection/cache状态机。全部共用可用方法代理；新增queue hook不影响Native（其默认本就无派生queue key）。仅测量example使用async-trait dev依赖，已锁版本不变；不得覆盖baseline Cargo原有功能。

独立full200 groundtruth放在cold首可用/本轮测量之后（或使用原公开seed证据），不能在首次child snapshot前预热。必须EOF、稳定owner/cut/revision/sourceCall；原始页面union与完整groundtruth逐条等价，before/after恢复不改内容、ID和顺序。记录实际产品入口调用链，独立groundtruth不能自证ChildRun确实调用了页面。所有额外refresh/prefix-validation真实调用都保留并计入对应产品操作成本。

## 每轮执行与计时

1. cold intent在connector/runtime setup前；其余轮intent在新ChildRun open前。公共父RunController保持独立scope与非空草稿，查真实catalog为原唯一bounded child。真实ChildRun open+transcript直到正确、非stale的child首可用；包含实际重复读取/刷新成本。
2. common50另做真实SEP初20/Earlier20→既有projection，测Earlier请求到完整新投影；不得slice50。product20直接测真实ChildRun Earlier20→transcript完成。所有页保留冻结cut/revision/原请求与完整entries。
3. 两种产品路径都实际load_earlier并保留该ChildRun完整已加载历史，不能只读head。common50实际50 Earlier仅报告实测成本/功能，不新增一套未冻结性能预算。
4. lifecycle保留本轮同一ChildRun跨Runtime.reconnect，打开原父、验证catalog与新endpoint/epoch，调用原child.transcript，验证此前加载历史/父草稿/sourceCall及来源水位恢复。不是重新new ChildRun冒充旧实例恢复。warm不reconnect，不更换consumer/Kernel PID身份。
5. full200与实际调用来源校验、字节/61read对账、provider计数不增长和父草稿隔离。记录原样before/after projection及原页；不靠transport ready或ack当作历史可用。

每个延迟记录原monotonic意图/完成时间；调用日志/验证若在该产品操作内发生，不扣除开销。snapshot10s、SEP/product20 Earlier1s、reconnect5s；common路径相对1.2，sampled RSS相对1.15及512/2048MiB，cleanup10s与原相对规则均保留。nearest-rank p95，失败保留，缺轮不补样。

## 运行时与磁盘

复用公开seed helper、owned_native、sealed closed seed/restart、50ms ProcessSampler、原machine/power观测与保守OS cleanup算法。cold公开seed后退休seeder与Kernel，原owned状态sealed，换新Kernel/consumer PID恢复同一pointer；OS页缓存不清，明确披露。warm/lifecycle同一Kernel与consumer，provider独立记账。sample gap>250ms失败。

主会话串行Cargo，对每角色批次开始前准备源码时间戳并记录、核对内容SHA；唯一SSD shared-target，不在快照内建target，不把历史binary回灌target。保存原失败、源码/WIP/固定快照、所有raw/log/receipt与程序。不改既有frozen profile，新增本family recipe作特定拓扑的受限解释，预算/样本数不变。

## 复核与结论

只读advisor已确认：common50+SEP20不能替代candidate真实ChildRun20，因此上述独立product20必须采齐；透明Endpoint wrapper是来源机制，不能仅用第二份SDK读回证明产品调用。每条路径独立报告，不混算observer/产品path、cold/warm/lifecycle或不同角色。即便本family范围通过，也不关闭#56整票、#58/#59或宣称阶段已交付；Pi/DSH Family、附件/队列和原有效性能失败继续保留。

## Observer ABI v1（实现与runner共用）

stdin JSON：`familyFormat:1`，`mode:cold|warm|reconnectLifecycle`，`path:common50|product20`，`role:baseline|candidate`，`caseIndex`，`connector:{identity:native,url,bearer,expectedVersion:0.9.3}`，`parentSessionId`，`childSessionId`，`sourceHistory`（本case公开seed full200原响应），`parentDraft:AKFAMILY_PARENT_DRAFT_KEEP`。product20仅candidate且binary必须启用acceptance-native-family-page20；其它组合在连接前失败，不fallback到50。

事件均JSONL，所有本轮事件含round（cold0；warm/life0初始化、1..10测量）/measured/path/role/parentScope/childId。稳定的核心事件名：

- `family_consumer_started`（host/version、模式、编译开关、readOnly、进程语义）；`family_iteration_started`；
- `family_first_snapshot`：intentMonotonicNs/publicationMonotonicNs/latencyNs、真实transcript、父draft、sourceChecks；
- `family_history_page`：机制`sep20-projection|childrun20`、requestedMessages20、原head/earlier请求响应、完整union/projection、同样时钟与sourceChecks；
- `family_product_earlier50`：实际ChildRun50功能及实测耗时，不能称20预算通过；
- `family_reconnect`：原ChildRun跨epoch、endpoint连接代号、before/after完整transcript、指针/父草稿/来源/已加载历史不变，时钟与converged；
- `family_groundtruth`：本轮测量后独立full200，原响应与seed匹配、两页union/sourceCall/EOF验证；
- `bounded_call`：每次真实SDK调用的原request/response或error，连接代号、phase与原时钟，始终原样转发；
- `family_iteration_completed`或`family_iteration_failed`，失败后依赖轮不运行；`cleanup_intent`、`runtime_cleanup_completed`；全局`consumer_error`。

公共调用记录保留全部refresh/prefix-validation；每轮内存scratch可以释放，原JSONL不可重写。failed case必须尽力stop Runtime，外部runner最终核对consumer与Kernel全owned树退休。任何sourceChecks失败都保留负结果，不生成success样本。完成正式注册前不可启动本observer或harness。

## pilot原始失败与隔离边界（历史证据）

先完成一个明确diagnostic的raw→报告pilot，再启动正式21-case矩阵；登记仍执行完整程序/源码/driver/运行时/recipe SHA校验。`--pilot`必须匹配`purpose=toolchain-pilot`和`diagnosticOnly=true`；原登记的official字段只作为通用执行登记格式，原执行收据/报告必须official=false，不进入正式聚合，不允许后续升级。实际收据与遗留缺口见[程序/pilot绑定](evidence-bindings-20261026/native-family-observer-programs.json)。

v3已实际走通candidate20产品首快照、Earlier、完整source和清理；离线修正报告器的重复初读、合法1条raw Earlier和原catalog关联后，源码/协议对账没有剩余失败。整体pilot仍失败：原收尾校验漏传pilot标志；RSS采样在显式注册consumer root前曾将同PID/start暂归provider，最终census角色改变。保留原收据与raw；修复runner标志后优先独立离线重验原pins，采样身份/角色要先消除歧义，不靠忽略RSS门禁把pilot判成功。原程序和数据不重写，正式矩阵尚未启动。

### v3独立离线补证完成（原始采集不改写）

派生报告`native-family-pilot-v3-offline-full-report-v2.json`完整通过raw source/真实产品快照/分页、原始事件、固定输入、OS身份、50ms RSS与cleanup校验。原receipt保持failed/identityRechecked=false；新proof用原冻结runner加正确pilot参数独立只读复核，绑定原registration/程序/来源闭包及原文件SHA。唯一RSS role更正只涉及同一明确consumer root的早期32KiB样本，核对PID/start/executable、注册provider父、spawn与首boundary时间、后续boundary和正常退出；未知进程、后期角色冲突或身份缺口仍失败。原/重算/保守峰值相同（47568/20064/88208KiB），max gap56.535708ms。报告与proof首次解析字节逐项SHA对账，全部原文件与时钟不变。

派生status=success仅代表本pilot完整闭环；official=false、diagnosticOnly=true、performanceAccepted=false，不能进入正式矩阵。采集工具已使Popen→root登记与归属分类互斥，不持锁等待readiness或进程完成；原50ms/gap250ms门槛不变。正式矩阵使用新固定runner闭包；之前cold/ordinary性能失败不因此改变。

### 正式子矩阵最终结果

[21-case正式绑定](evidence-bindings-20261026/native-family-official-cohort.json)与独立只读复核证明：common50+SEP20原适用绝对/相对门槛通过，candidate20各适用绝对门槛通过。21个case各采集一次；两个材料器误判按time:f64契约只离线纠正原raw，原负报告保留；v1/v2/v3仅post-capture reporter不同，采集器/observer/程序没有变化。原冻结JSON的preregistered-not-run字段是不可变登记格式，不代表当前执行状态；最新状态以#14正文和正式绑定为准。ordinary有效失败及其它端/附件/队列不因该子门通过而改变。
