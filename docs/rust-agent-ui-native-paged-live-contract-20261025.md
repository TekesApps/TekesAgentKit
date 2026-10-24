# #17 / #23：真实 Native 累计分页来源执行契约

实现起点4f2c02b，关联#27、PR#10。沿[累计来源契约](rust-agent-ui-native-paged-pane-source-contract-20261025.md)继续真实协议验证；不修改原head-only profile或降低其断言。ClientKit只读行为基线c289bc9、Tekes只读8858ccd，Kernel0.9.3/09508a9与TekesUI公开精确50ec49f保持不变。

## 公开协议与种子

Kernel0.9.3 history_page先按append-origin消息划下界，再扩展kernel_seqs/sourceEventSeqs因果闭包。maxMessages=1不是事件数或返回消息数的硬上限。projection.rs的step/end可以连接同turn前后attempt，单纯增加同turn消息不保证增加页数。独立只读审查确认这与实际公开返回一致，不为凑页数修改Kernel或切割事件。

隔离provider在原Task child中通过公开read执行四个普通步骤，每步产生NATIVE_PAGED_STEP_1/2/3/4助手文本并读取owned proof.txt；第五次child模型请求保持门禁。parent仅授予task/read及owned workspace读范围，禁用network，写/工具链roots为空。read必填path/offset/limit从公开schema推导；续轮只按本夹具已分配的previous_response_id/function_call_output.call_id关联，并检查前次真实read回传，不落盘body/凭证。实际Kernel执行工具并产生历史，无journal注入。

初始Task指令不会产生child user/message，因此marker关联sourceCall.arguments，四个append-origin assistant内容分别检查。旧尝试1/2未完成工具链，3未提供宿主第二main槽，4要求三次Earlier但实际因果闭包仅两页；全部失败证据保留，不作为通过。

seed用公开接口创建parent及auxiliary main槽，客户端退出后由独立公开subagents.list/subagents.history取得child pointer、full head(max200)与one-message head(max1)。按同一cut/revision、严格递减beforeSequence继续逐页遍历，保存每页请求/响应，要求非空进展、无重叠及拼接精确等于公开full head。最多17页；所得pagePlan仅作独立oracle，不输入pane/重建projection。成功Earlier次数必须精确等于pagePlan.len-1，不预设为三次。

## 原控件与帧

manifest profile=native-paged，包含parent/childId/marker及expectedEarlierReads；CLI显式--child-pane-paged-source，组合原--child-pane-source-proof/receipt与offscreen/feature。旧profile不变，无feature拒绝。

原Sidebar/FamilyCatalog/OpenChild创建同一个page_size=1只读pane。每次显式读必须由原ReadOnlyTranscript Refresh/Earlier产生DemoCommand::ReadChild，校验source Arc/parent及原intent，执行read_intent_with_paged_source_witness，再经原HostResult::ChildRead安装同次snapshot。禁止普通snapshot后另取见证、手造intent、直接load_earlier。原宿主自动刷新允许发生，但其实际typed收据必须完整记录；旧Earlier拒绝不能产生额外Earlier。

按顺序输出AGENTKIT_NATIVE_PAGED_SOURCE帧：

1. running_head：原Refresh，hasMoreBefore=true，精确等于独立one-page head。
2. 从原Earlier控件捕获未消费intent，显式runtime reconnect/waitReady；同source/pane，旧Earlier必须报epoch expired。原view错误完成路径清除loading，再经原Refresh输出reconnected_refresh。新epoch返回，缓存页保留原readEpoch，不冒充经新连接全部重读。
3. reconnected_page_1..N：逐次原Earlier；请求frontier/cut/revision和返回entries逐项匹配独立public pagePlan，累计收据重放精确等于projection；最后hasMoreBefore=false并覆盖完整head。
4. 外部validator接受全部pre-gate帧后才放行第五请求的受控HTTP400。settled_refresh由原Refresh产生，要求runOpen=false/sourceCall.error、原prefix逐项保留且有新增错误/结算事件，新的cut/revision及未合并PrefixValidation真实记录。

每帧包含phase/parent/childId/marker/sourceInstance、读取前原intent(read/scope/publication/epoch)、witness(snapshot/source完整serde)、mountedSnapshot及mounted原ReadOnlyTranscript root、rowKeys/rowIds、parent draft/selection和source identity。rowKeys使用公开RowKey的Debug字符串，不新增公共serde契约。仍存在的同key ID稳定；装饰fragment合法重组不要求永不消失。snapshot为原PaneSnapshot的scope/publication/epoch/connection/transcript，mountedSnapshot必须与同次返回值一致。intent publication严格早于返回publication；Earlier epoch一致，Refresh允许从旧epoch更新。

## 终态与证据边界

原SidePanel Close退休source，旧witness拒绝；父draft/selection及原parent binding保留。host receipt的evidenceScope=actual-native-paged-child-original-intents，包含publicPagePlanMatched与实际earlierReadCount，不再使用threeEarlierReads。desktopInteraction/physicalIme/readingAnchorAcceptance均false。

runner仅消费显式prebuilt程序和固定安装Kernel0.9.3；复用SSD共享target，不编译、不新target。归档程序、固定源码输入、公开raw返回、帧/命令/退出证据。probe独立process group；stdout queue有界等待，超时仅回收所属group。demo及所属服务退出后才写最终accepted。

此profile若实际通过，证明该真实历史形状按公开协议分页至EOF、同pane/source重连与旧intent拒绝、追加回传及冻结prefix行为。当前种子若因果扩展只需一次Earlier，不能宣称三次累计合并或Earlier增加可见消息；这两项继续保留受控多页fixture/未来实际多turn种子的验证边界。公开oracle证明consumer与Kernel一致，不能独立证明Kernel算法无共同缺陷。行ID不等于阅读锚点：不据此关闭#18或完整#17/#23/#27。跨宿主锚点恢复仍#42，嵌套阅读仍#53，Kernel#86不豁免；不跑桌面，不改Kernel/Swift/TekesUI，完整Views/ProcessContainer/IME/OS AX/性能未验收。

Prefix元数据独立检查：hasMoreBefore/runOpen按旧cut的onePage核对；sourceCall的callId/name/arguments保持原身份，但phase是当前parent task元数据，错误后可推进到error，按全部串行读取结果要求running→error单向变化，不盲目拿旧running比较，不能盲目与旧running phase做整对象相等。公开实跑与冻结adapter源码均确认该区分；mutation测试拒绝错误分页、runOpen、call identity或phase。投影中的助手文本/源seq、工具参数/结果/元数据以及task状态也必须对应typed公开历史。

错误与turn/end可能分批发布：每次新head必须验证紧邻旧head的cut/revision及精确one-page prefix，包含中间cut；首个prefix仍必须绑定独立pre-gate oracle。不能假设错误后只有一次head增长。
