# Sidebar 真实 transport 中断与恢复契约

归属 #22/#27，关联 #15/#28、draft PR #10。基线29ea2e1。仅修改 AgentKit 的验收入口和工具，不改 TekesUI/Kernel/ClientKit，不跑桌面、私有history或付费模型，不新增业务状态机。

`SharedAgent.follow(state)` 观察内存 watch；transport 断开不会等同于本地 observer 终止。原 observer Retry 只负责重新订阅，不能用来冒充 transport reconnect。本切片验证真实 Native0.9.3 / Pi1.3.0 / DSH0.2.1-rc.2 的客户端 transport teardown、连接失败预算和宿主显式 Reconnect。它不证明服务器重启、网络分区、observer终态Retry、未知mutation结果重试或桌面/OS AX。

验收 example 使用自有 GatedConnector，返回真实 connector 产生的原 SessionEndpoint，保持原注册 runtime。先阻止后续connect，再对原endpoint调用真实close；不注入MemoryProjectionStore、Sidebar packet或伪造Failed。Native/DSH连接与Pi所拥有RPC进程按其原close关闭。blocked connect明确报错，等待SessionRuntime真实Failed；随后开放gate，经原ComponentDemo控制的合成Content事件取得DemoCommand::Reconnect，使用捕获的原endpoint runtime执行公开reconnect/wait_ready。该example是验证宿主，不把它称为NativeHost窗口循环。

逐端运行，维持原SharedAgent、Sidebar aggregate/host、ComponentDemo、RunBinding/controller、选中项与六个非空中文草稿。故障端所有原行的activity保留并标stale，其余两端仍fresh；observer自身仍loaded/converged且stale=false（该字段只表示内存观察生命周期），observer代数与原owner不变，不应出现observer Retry。健康端通过真实公开inventory refresh确认仍可接收基线。恢复须原scope的新connection epoch、新endpoint对象及合法inventory/journal基线，原host实际消费且行恢复fresh，原组件row IDs/挂载与草稿/选择保持。只见Runtime Ready不算Sidebar恢复。

原selected-read回执绑定其epoch，故障端旧回执恢复后仍拒绝；健康端同代真实回执应继续可用。Sidebar未提交action ticket的契约不同：同owner/observer代且地址/原定义未变时允许恢复后重验并首次执行，不追加“凡epoch变化全部拒绝”的新策略；stale、消失行、定义变化、旧owner/observer代仍拒绝。本切片可用只读Copy结果验证重验，不派发业务mutation；不把“本流程没有业务写命令或新增provider请求”扩大成不确定结果下无重试已验收。

runner复用原三端owned启动、公开seeding及显式配置，新增互斥opt-in，不改变默认/Task-family/SideChat流程。只运行显式现有程序，不编译/安装；每轮源码、依赖、程序、公开阶段收据和退出日志保留。独立Python verifier检查精确端点/会话集合、阶段epoch/代数、真实close/connect计数、source与挂载身份、原回调命令、草稿/回执和provider计数；拒绝手填accepted布尔或缺阶段的结果。

主会话串行构建/验证，统一使用SSD shared-target。先完成一个真实端点的关闭→Failed→原Reconnect→基线闭环，再扩展三端；失败保留，不因门禁不易通过而改成仅显式reconnect或模拟packet。共享包/demo/第二consumer按改动适用验证。桌面与SDK #53等已有门禁保持开放，不关闭完整Issue、不合并/tag。

## Probe / runner 收据接口（schemaVersion=1）

CLI：`sidebar_transport_recovery_probe --endpoint-config ABS --receipt NEW_ABS [--endpoint native|pi|dsh ...]`，缺省三端按native/pi/dsh顺序；配置仍是原HostEndpointConfiguration三端，每端两个scope；endpoint子集只限定故障case，不删其余健康端。runner新增互斥的 `--transport-recovery-probe PATH` 与可重复 `--recovery-endpoint`，仍复用原三端普通seeds；原`--ui-demo`参数保留兼容，但该opt-in实际执行的是显式example。选择不支持/重复端点直接拒绝。

顶层必须有 `schemaVersion:1,status:"completed",evidenceScope:"real-client-transport-close-runtime-reconnect",desktopInteraction:false,endpoints:["native","pi","dsh"],cases:[...]`。

每case：`endpointId,address,before,failed,recovered,healthyRefresh,reconnect,selectedReads,capturedDefinitionCopy`。三stage同形：

- `delivery`：直接复用 `sidebar_delivery_receipt` 的原 source/display getter收据，不用 from_facts 手造。
- `sourceActions`：与该stage的delivery使用同一个原host publication提取的九行 `{address,actions}`；常规delivery getter省略菜单，此字段保存实际canonical定义，verifier据此绑定Copy的ID和文字。
- `runtime`：三端map，每端 `{connectionPhase:"ready"|"failed",connectionEpoch,inventoryReady,selectedJournalReady}`；只收最终稳态，两个connecting间态不冒充failed。
- `mounts`：`[{address,rowId,mounted}]`，来自原组件row_id与实际ContentHost节点检查，不能仅据getter称已挂载。
- `selectedScope`：原 `view.current_binding().scope()`；`drafts:[{scope,text}]`：六个RunBinding的实际snapshot；`observerRetryEndpoints:[]`：逐端查询原Sidebar Retry控件。
- `gate`：故障端真实控制器snapshot `{attempts,successes,rejections,closeCount,blocked,connectionGeneration,activeEndpoint}`。初始successes=1/closeCount=0，failed关闭1次、拒连>=4、successes仍1，恢复successes=2、closeCount=1、代次增加、blocked=false且activeEndpoint=true。`closeCount`只统计gate显式调用原endpoint.close的次数，不包含Runtime内部清理。`connectionGeneration`为实际成功连接次数。若实际runtime行为与此不符，先报告，不伪造计数。

`healthyRefresh` 是两个健康端map，值 `{epochBefore,epochAfter,revisionBefore,revisionAfter}`，实际公开refresh后epoch不变且该child publication revision前进。

`reconnect:{commandCount:1,endpointId}`，只统计原RECONNECT控件产生的DemoCommand::Reconnect。允许并实际执行原ReadInitialFeedback的request.execute()/complete_initial_feedback等只读背景回调，不能抑制它或把它算成Send；遇未知业务写命令fail closed。例子宿主将Reconnect委派原runtime，与NativeHost采用同一公开reconnect/wait_ready路径；不声称执行私有NativeHost::commands。

`selectedReads:{selectionGeneration:1,selectionGenerationOwner:"probe-selected-read-scope",faultAcceptedAfterRecovery:false,healthyAcceptedAfterRecovery:{healthyId:true,...}}`：原host.read_selected、project_into在相同selection_generation/原地址上实际验证；不能仅写布尔不调用。此处generation由probe的独立selected-read scope拥有、前后保持1，只证明同代下owner/address/connection epoch fencing；不声称读取或验证ComponentDemo私有的选择代数。

`capturedDefinitionCopy:{definitionId,expectedText,actualText,address,rejectedWhileFailed:true,executionsAfterRecovery:1}`：故障前从原行捕获enabled copy定义，在Failed时经原host.perform_action拒绝；恢复后通过同原host回调首次重验并得到原地址的Copy outcome。此项保存的是原菜单definition；原host在调用时按当前publication捕获action ticket并重验，不声称持有并跨重连提交同一份旧SidebarMultiActionTicket。它不增加旧ticket强制epoch退休，也不写OS剪贴板。

verifier独立对照seeds：source/display/mounts精确九会话，原title/cwd、同地址同rowId、选中/六草稿不变；各case阶段revision推进，原observer仍observing且generation不变，只有故障端stale/Failed，恢复所有fresh；epoch、gate计数、健康端refresh和回执/Copy范围必须一致。runner另记录recovery前后provider请求数量与路径完全相同、owned进程退出，之后才写accepted。它仅证明本流程未发新增模型请求，不扩大为未知mutation结果下的无重试证明。

初始Sidebar默认每页5行；收据要求九行实际挂载，因此probe先通过原Show more控件把普通库存展开到九行，不能改私有limit或伪造rowIds。顶层另记 `bootstrap:{pageActions:正整数,mountedCount:9}`；这只是本profile的准备，不提升完整分页/reveal/像素验收。各stage source/display仍须完整精确九会话，选中与排序可变化但同地址rowId不变。
