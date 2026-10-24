# #23：同宿主真实 Task / child-family 接入契约

## 目标及现有基础

复用 `9b9ec1f` 的共享三端 lifetime、受控 provider 和同一当前 demo，不增加协议/导航状态机，不接桌面，不修改外部仓库。源要求仍按 #23 的全部标准保留；本契约只安排其中可离屏验证的部分，不降低关闭标准。

Python 经公开 API 生成真实 source-call、catalog 和 session 身份；demo 比对实际 parent Run 的 canonical SummaryFacts 与实时公开 catalog，通过明确的 FamilyCatalog/SidePanel 原 intent、background command、结果分发及既有 ChildPaneBinding 打开子历史。Native 父 scope Task 不伪装为可导航的独立 Task；TaskPaneBinding 的独立 Task 契约保留单独验证。不得由验收脚本直接构造 AgentTask、修改 task.scope、写私有历史，或借用单端 selftest 伪装三端来源。

## 不可合并的来源语义

| 来源 | 应消费的真实事实 | 不得推断 |
| --- | --- | --- |
| Native Task | 实际 task call 的 canonical Summary task，scope保留当前父Run；原OpenTask到TaskPaneBinding | 不凭工具调用推断独立可发送session pointer |
| Native bounded child | 真实Task/catalog产生的child，原ChildPaneBinding只读有界历史 | 不伪装SameSession Run，不挂Composer，不开放发送 |
| Pi child | 原公开subagent create/prompt/read及canonical catalog / Summary subagent | child不等于Task row，存在session ID不等于有控制权限 |
| DSH child | 真实subagent tool /公开catalog及独立child来源 | 不把todo或parentSessionId直接当child pointer；未知todo schema不注入 |

现有 `ComponentDemo` 从各父Run `SummaryFacts.tasks` 更新TaskPaneBinding；直接复用。Native task和DSH todo在当前presentation保持父scope，Pi子代理未证明产生Summary task。真实**同一Task row动态改指向**的公开producer尚未找到，保留注入来源的既有A→B→A契约测试，不能用真实家族导航替代它，也不把该缺口错误归给TekesUI。

## 同帧证据与动作

每一步保存实际消费frame的完整parent/target RunScope、connection epoch、source revision、真实tool call ID/catalog ID及来源序列；UI revision不能代替这些身份。记录原请求的parent/task/child/revision，以及原dispatch接受或丢弃；记录实际SidePanel selection、Task binding access、挂载Run scope或只读source及控件节点。

用两个真实parent/child来源进行家族A→B→A、返回父级与关闭；保留canonical Run和原草稿。受控延迟只能延迟真实返回结果在原dispatcher中的投递，注明“受控投递延迟”，不声称真实网络发生乱序。旧结果不得重新挂载已退休pane；关闭本地tab不得发送远端cancel。节点挂载不代表桌面可见、物理IME或AX通过。

## 分工与验证门槛

- 新seed子会话：仅公开seed命令及真实返回值到来源manifest的解析/拒绝测试，不改demo、启动器或产品状态；不执行harness。
- 主会话：审查来源，负责原intent/dispatch接线、同进程只读收据和串行真实验证；所有Cargo使用已有SSD共享target。
- 只有实际来源满足上述约束才回填对应细项；同Task row真实动态producer、窗口/chrome、物理IME/AX仍须单独证据。#53严格门槛保留，不合并/发布。

## 2026-10-24 入口审查修订：保留Task语义，新增明确catalog入口

当前ClientKit Summary仅为独立session的Task生成导航；AgentKit父scope Native Task没有OpenTask action是正确行为，不能为验收强行打开它。现有客户端路径没有隐含的Native bounded入口。改为补齐AgentKit自己的受控 `FamilyCatalog` 高层控件，组合在SidePanel旁，输入原父Run公开catalog publication，输出捕获父scope/child/generation/epoch/revision与控件实例的打开意图；底层按钮/布局仍由固定公开TekesUI提供。它不把bounded child塞进Summary subagent，不修改TaskPaneBinding，也不从sourceCall拆opaque ID推断Task指向。

来源适配复用presentation现有ChildCatalogState的begin/commit与既有父Run authority/epoch fencing；不新建catalog状态机。失败或父owner退休撤销可用意图，显式refresh只读获取公开catalog，节点本身不授予发送权。Host采用原OpenChild→HostResult→install_child_source，Native SameSession仅挂原只读Transcript；Pi/DSH IndependentSessions仍由原binding/catalog决定control availability。

控件输入约定为不透明source owner key、parent、connection_epoch、generation、revision、refreshing、catalog、error；每个widget只保留当前输入和原SDK Content节点/捕获身份。每个parent/source内同version冲突与旧代回退均拒绝；独立parent或重建owner的版本计数不能互相比较。切换source/parent必须退休原控件捕获身份，跨source、scope或instance旧意图均拒绝。catalog中的parentAvailable和mode原样保留，不能将“可看历史”当成“可发送”；只读历史不因没有发送能力就凭空消失。

新入口须在同一真实三端程序验证可发现原控件→原意图→原child绑定，逐端两个真实parent/child间导航、返回、关闭、受控迟到与草稿保留。Native Task保持非独立行，真实Task动态pointer单独未验证；这仍属于#23既有真实家族入口标准，不重复建票。设计与来源声明须回填该Issue。
