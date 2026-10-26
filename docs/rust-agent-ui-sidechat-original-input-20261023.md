# SideChat 原编辑器输入与目标脚注复验

本轮只修改验收输入和严格判定，没有修改 AgentKit 业务组件/绑定或 TekesUI。
前一阶段通过真实 fork 的 controller 直接写草稿；现在 Native/Pi SideChat 必须通过
原 Composer 的 SDK Preedit → 被阻止的 Send → Commit → 原 Send 进入真实 endpoint。
preedit 使用中文，要求 marked 期间不产生业务 Send、仍保留原编辑器 focus/组字；
Commit 后仅通过既有 Edit intent 写入对应目标。重复 open、发送、草稿导航、重连不重复提交、
重连后 open、原关闭与确认丢弃继续严格验收。没有新增输入/IME/焦点引擎。
`originalEditorFocusRetained` 对应组字门禁阶段，不扩大为完整原生焦点路线验收。

Native Kernel 0.9.3（明确 UI-only）与 Pi 1.4.0 的该原组件离屏路径通过，
`originalEditorInput` / `syntheticPreeditBlockedSend` 均为 true。
DSH 0.2.1-rc.2 仍缺少原 discard 能力，仅原禁用门禁通过，不能宣称支持 SideChat 闭环。
验收程序 SHA-256：`61081185b5287e0914fbac58c8faf5be0c169782ba4aeed228ebabc13c1ee4fa`。
程序以 APFS copy-on-write 克隆归档，是独立 inode，不是会被后续 Cargo/签名改写的硬链接。

相关组件测试 **7 项**（含第二公开 consumer）、Python **74 项**、严格 workspace/all-target
Clippy、格式和 UI 边界通过。本轮未重跑完整 UI；基线 `9217824` 的 623/1 既有忽略保留原收据。
业务组件和绑定源码摘要在 JSON 中单独保留，不把旧全量结果称为本轮重跑。

## 严格目标脚注仍失败

先复用了前一阶段程序 `656ff239…c76c6d`，没有为该复验重编或复制程序。
真实 Pi 1.4.0 的公开 create/pause/get/clear 数据链、原 footer 的 scope/id/revision/phase、
clear 后 reconnect 新 consumer 不再显示旧目标均通过，但 `labelVisible=false`，严格命令退出 **1**。
保留 `dataLifecycleAccepted=true` 与 `footerVisibilityAccepted=false`，没有放宽门禁。
这是两个新 consumer 的离屏证据，不是同一已挂载视图的流式更新。

实际 1068×760 自绘 PNG 中，窄 output aside 的复制/评价/分叉文字按钮占满单行，
时间/目标越出 clip；没有再把早已移除的常驻反馈 loading/Refresh 作为当前原因。
ClientKit 的 compact 动作需要已公开的原 glyph/紧凑内容服务，继续由 TekesUI #47 跟踪；
不隐藏动作、改成多行、裁文字假扮图标或复制 renderer 来通过验收。
本次未补发内容相同的 issue 评论。当前公开 pin 仍为 `50ec49f`。

原日志、收据、离屏 PNG 见 `.runtime-fixtures/real-goal-footnote-current-20261023/`；
原编辑器专项的程序/日志/源码与锁快照见 `.runtime-fixtures/sidechat-original-input-20261023/`。
两份程序身份与结论分别记录，不能转移证明。

所有新增输入是离屏 SDK 合成事件；没有桌面窗口、屏幕截图、签名安装或权限申请。
物理 IME、VoiceOver、完整阅读锚点和 ProcessContainer 同/跨进程仍未验收。
继续复用共享 target，没有新建 target 或删除证据，实际释放 0 字节。
