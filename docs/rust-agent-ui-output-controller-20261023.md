# OutputMessage：原控件与消息控制状态分离

基于 `d2a5568`，将原 OutputMessage 的 canonical identity、scope、publication/epoch、header revision、来源快照、part facts、单调 ID 分配和 thinking stamp 移入私有 `OutputMessageState`。它不持有 Content 或 StackContent。原 composition 仍唯一持有实际 TekesUI 节点，更新时将该节点集合显式借给原消息算法。

`thinking_stamp`、`update_display`、`check_update_display`、`reuses_part` 继续使用同一份实现；来源、冲突、过期、文本版本耗尽和 namespace 耗尽门禁均保留。逐项文本比对在显式节点参数替换后四项一致，记录在本阶段 `controller-algorithm.json`。这只是迁移一致性证据，运行行为另行验收。

公开 OutputMessage 构造、更新、查询、copy/action、into_host 和 Component 接口不变。root/part ID、ToolRun 成员复用、原 editor、selection、局部 disclosure、布局 tokens 和 AX 语义仍由原控件承担。没有改 TekesUI、Kernel 或 Swift 源码，也没有增加公开的宿主消息状态机。

这一步解除“消息控制状态必须和连续的一段控件行同处一个结构”的限制，为后续实际分区提供同一套更新入口。**物理 Activity/Output 分区和 settled disclosure 尚未完成**：Transcript 仍安装原 OutputMessage；没有把这个内部拆分算作 ClientKit 布局验收。

接下来的分区需让 turn composition 持有互不重复的实际 part 节点，在结算后分别安放 Activity 与 final Output；调用本阶段同一个控制器完成来源校验和原节点更新。展开选择属于 AgentKit 本地展示状态，需按 scope/turn 和原 prepared-plan 门禁保存；阅读注册与原文本 owner 要随同实际节点保留。正式 ProcessContainer 同/跨进程仍等待 TekesUI #48。

验证范围和精确程序、日志摘要见 [机器记录](rust-agent-ui-output-controller-20261023.json)。UI workspace 591 项通过、1 项原有 #49 忽略，含组件、demo 宿主和第二公开 API consumer；UI 全 targets 严格 Clippy、格式和依赖边界检查通过。根源码未变，根 289 项沿用上一提交的独立收据，不称为本阶段重跑。

三端原生公开历史分页另通过前插/向上阅读锚点、A/B 阅读与草稿、重连恢复；Pi 50→56、DSH 50→59、Native 50→56 个加载项。`nativePagingControl=false`，请求通过公开 page API 触发，不算原生分页按钮验收；各 profile 未验的 false 字段完整保留。两条三端 driver 均退出 0。

当前 UI 冻结程序 SHA256 为 `20314521dadde53b0c29bea9af771c700a5a0648cb712cd2e1117b7c69f3ddb9`，搭配同阶段 headless 程序。真实 Pi 1.3.0、DSH 0.2.1-rc.2、Native Kernel 0.9.3 的原生 stream 和 stream-reading 各自通过，保留原输出/文本 owner、选择区/焦点、追加期间阅读位置、草稿隔离及重连无重放；provider 为受控本地合成 HTTP 服务。stream-reading 使用 synthetic preedit/wheel，不能代替系统拼音。

相同冻结程序另通过一次正常 LaunchServices 系统拼音探针：系统 Preedit、组字期间发送/导航屏障、Commit 和单次提交；输入源前后不变。PID 定向软件按键、physicalKeyboard/candidateWindow=false，未验物理键盘或 VoiceOver，先前 #41 的首键异常仍保留，不宣称稳定性已证明。

本轮再次确认已发布 AgentKit 为 v0.3.8，TekesUI 公开 main 仍为精确固定的 50ec49f。d2a5568 的两项基础 CI 成功，两项 UI CI 失败；已读取失败日志，严格 bounded_detail_reading (#53) 未放宽。PR #10 保持 draft。Native 明确使用 kernel-ui-only，不包含完整 lifecycle 或 bounded-child 协议验收；运行 harness 子进程不代表 ProcessContainer 跨进程 UI。

复用 `/tmp/agentkit-layout-text-292-target`，关闭 incremental，新建 target 0；冻结程序使用硬链接。保留源码、失败记录、依赖快照、程序、截图、日志与收据，未删除共享缓存，释放空间为 0 字节。
