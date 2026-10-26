# #25：DSH Summary 标题与同帧完成来源

只改AgentKit；复用当前presentation与固定公开TekesUI50ec49f。ClientKit c289bc9和Tekes8858ccd只读，保留WIP；不改Kernel/SDK/DSH，不接桌面。

## 已确认的问题

真实DSH0.2.1-rc.2公开assistant/message tool-call的arguments是JSON字符串，内含description="Catalog proof child"。baseline复用858a63c已归档程序；其Summary源码与7e9383b完全相同，按对象直接索引而显示Subagent（不是声称baseline程序构建于7e9383b）。当前ClientKit SummaryPanelView的SummarySubagentPresentation会解析一次JSON对象并取brief/description；缺省时用原call.headline，headline来自SessionEventProjector.toolRowTitle。

## 最小实现

Summary只对task/subagent/subagents读取局部参数对象：原object借用，字符串仅解析一层且必须是object；不递归解码，不重写原Transcript/history。Task只取task_name，Subagent按brief→description trim；缺少有效标签时复用既有TranscriptToolCall.presentation_title（即原producer headline规则）。坏参数仍保留原Subagent行；不凭title/参数生成pointer或状态，不改变同端inventory授权。无新增状态机、接口或基础UI控件。

## 可执行验收

- Rust回归：真实DSH字符串参数形态、object等价、优先级/trim、坏JSON/非object/双重编码、原参数不变、producer headline的call/result envelope与优先级、同端pointer/status门禁。
- 现有默认/feature presentation、根workspace、UI包与普通demo及独立公开consumer按适用范围验证。仅复用SSD共享target，不为阶段创建target。
- 真实DSH runner显式消费已构建feature headless和固定DSH安装，owned_dsh关闭query_sqlite插件，隔离HOME/data、合成loopback provider。原工具创建child、continuation、公开childHistory、重连前后原publication捕获；不读私有session文件。
- 最新child inventory没有raw status；显式DSH验收profile只接受同次authority中数值seq最新turn边界为turn/end reason.kind=completed，且非运行/非blank、无队列/parked/actionable。不把父tool.phase当子status，不将完整活动算法复制到fixture；Pi默认验收边界不扩大。原缺省字段保持缺省。
- 原SummaryView/ContentHost消费原facts，标题须Catalog proof child，原scope/action和completed→无标记一致；只证明挂载与语义，不证明glyph/像素/hover/桌面/IME/AX。

同帧见证证明内存构建输入一致，不能证明服务端多网络流事务一致或抵抗完整producer伪造。运行前原始baseline保留；最终程序、原witness、source/hash、commands/stdout/stderr、正常退出与回归日志归档。#25/#27不据这个切片整体关闭，#53严格门不删减。实际结果另记，不以本契约当通过。
