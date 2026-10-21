# 已发布 SDK 的 Run 阅读事务

后续见[保留会话阅读与真实三端分页](rust-agent-ui-retained-reading-20261021.md)：已修复重连 stale footnote 跳动，并取得真实三端分页/A-B阅读/重连原生收据。以下保留初次接入时的证据边界；新建 host 的持久恢复另跟踪 TekesUI #42。

AgentKit 继续精确消费公开 TekesUI `1294987dacabb33c0e411c13ee5ea544a1123f8e`，不修改 SDK。#22/#28 的 issue 状态仍为 open，但公开检查点已提供 content reading 与 item reveal；不能再把 AgentKit 接入工作归因于 SDK 未发布。

## 接入契约

`publish_run_reading_update(host, root, original_version, display_revision, plan)` 接收 RunView 现有 prepare/validate/apply 计划。先校验原 Run 的 scope、publication generation、编辑器 baseline、草稿 authority 和 TekesUI controlled-value 冲突，再进入原 SDK reading 事务，同步发布 Transcript 与 Composer。旧 token、外来编辑器、迟到发送清空回执和组字冲突都在回调前拒绝。无异步让出、编辑器克隆、像素补偿或第二套 Run/阅读状态机。

display revision 严格递增，独立于 provider publication；连接、可用性和组字门禁可在同一 provider revision 下变化。SDK 原 token 始终由当前原 viewport 提供，remount fencing 由 SDK 保留。公共组件不替换宿主文档，也不请求焦点或隐式恢复到底部。

正式 ComponentDemo 首次安装 Transcript 时声明全部稳定行 ID；后续成对更新走该公共 adapter，去掉原来的重复 Transcript 更新。主 Run 和有写权限的 SidePanel Run 复用同一路径。只读有界子历史仍走其原组件路径，本次不宣称其原生分页锚点通过。Sidebar 自动 reveal 仍是独立的显式导航附件，尚未改成完整库存持续阅读文档。

第二 public API consumer 暴露 `publish_original_run`，正式回归经此 wrapper 调用共享实现；不依赖 demo 内部状态。

## 验证边界

CoreText 回归覆盖追加/前插时保留阅读行、Composer 高度变化、preedit 期间同值 authority 的追加、旧 reading token、编辑/undo ABA、外来编辑器、迟到发送清空回执、权限关闭再开启的 ABA，以及同 provider revision 的可用性刷新。阅读位置按实际可见文档的相对位置验证：分页状态栏改变高度时，固定窗口坐标不是正确基准。

完整 UI workspace 411 项通过（含独立 consumer 42 项及新增 reading 9 项）；fmt、all-target 严格 Clippy、26 项 Python 验收工具测试、依赖边界和实际 Cargo 解析的公开源清单检查通过。

原生探针 `--components --transcript-content --reading-native-selftest` 使用独立本地 SEP，通过原分页按钮和原请求发起一次读取，在回传前经 NSTextInputClient 开始合成中文 preedit，再放行历史页。检查原阅读行及其视口相对位置、向上阅读状态、编辑焦点和 marked baseline；连续同 publication 刷新不改变它们，历史增加但无新提交。

此探针是真实原生/GPU 布局、公开分页协议及合成 wheel/preedit 证据，不是物理键盘、真实输入法或三 harness 分页锚点的替代。真实系统拼音、三端原生流式/取消和 Sidebar reveal 的回归收据分别记录。

原生窗口的系统拼音检查：离散 nihao 按键产生 ni hao marked；Enter 只结束组字并保留 nihao，没有发送。继续以空格提交你好；B 输入我，A/B 分别恢复 nihao你好与我。未改输入法设置，未通过 PreeditRange/Commit API 注入。另以真实窗口滚动及分页按钮加载，历史从20增到44条，原可见回答21仍在阅读区，B草稿我保留；此手动检查保留AX与前后截图，不替代探针的相对位置断言。

Native Kernel0.9.1、Pi1.3.0、DSH0.2.1-rc.2 均通过公开协议、查询/库存、组件数据回传和两组原生回归：流式单次发送/输出身份/选区/焦点/草稿/重连防重放；发送/取消/错误恢复。provider为隔离受控服务，无收费调用或用户配置修改。Native首次窗口失活错过active preview的超时保留；独立新夹具重跑通过，未重放原请求。三端收据仍明确readingAnchorAcceptance=false。

Pi 流式探针曾把窗口失活后的空焦点误报为发布丢焦点。事件日志确认点击先取得原文本焦点，随后收到原生 Focus(false)；生产宿主按既有规则保存目标并清空活动焦点。探针现在等待真实激活后再断言原焦点，保留45秒超时；未添加强制焦点恢复或通用输入补偿。原失败和详细事件日志保留。

完整原生阅读验收仍需覆盖三端历史窗口的不同能力、重连锚点、跨挂载会话阅读恢复、只读子历史及 Sidebar 全库存持续阅读。UI 保持实验状态，PR10 保持 draft。

## 验收资料与磁盘

日志、原生程序 hash、系统 IME 记录及检查结果保存在 `.runtime-fixtures/agentkit-continuous-reading-evidence/`。初始探针的固定窗口坐标断言及错误的“单页即完整30轮”夹具假设已校正；失败日志保留，未据此修改 SDK 或弱化原业务不变量。

分页探针等待原生事件和异步页回传时不再空闲忙绘制，只在阶段变化及原观察者通知时请求帧；终态相对行位置/焦点/marked baseline收据通过。窗口在20秒内未激活仍会按原超时失败，不计成功。

全部构建复用 `/tmp/agentkit-layout-text-292-target` 与现有协议测试 target。保留源码、固定 SDK 快照、日志、收据、截图和验收程序；不新建 target，不整体删除共享缓存。
