# #23：同宿主真实 Task / child-family 接入契约

## 目标及现有基础

复用 `9b9ec1f` 的共享三端 lifetime、受控 provider 和同一当前 demo，不增加协议/导航状态机，不接桌面，不修改外部仓库。源要求仍按 #23 的全部标准保留；本契约只安排其中可离屏验证的部分，不降低关闭标准。

Python 经公开 API 生成真实 source-call、catalog 和 session 身份；demo 从实际 parent Run 的 canonical SummaryFacts 获取来源，通过原 Summary/SidePanel intent、background command、结果分发及 TaskPaneBinding/ChildPaneBinding 打开。不得由验收脚本直接构造 AgentTask、修改 task.scope、写私有历史，或借用单端 selftest 伪装三端来源。

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
