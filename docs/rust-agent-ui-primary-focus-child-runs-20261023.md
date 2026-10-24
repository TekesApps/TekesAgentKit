# 主按钮焦点与真实可继续子 Run

本轮修复 Rust Composer 的实际差异：Send/Stop 原来使用普通按钮的激活焦点策略，
Stop 被 Send 替换后，后续输入可能不再进入原编辑器。ClientKit 的主按钮拒绝普通
first-responder 接管；Tekes 的 KeyboardFocusRoute 通过独立显式路线聚焦该按钮。
核对 ClientKit `c289bc9` 的 ComposerActionBarView 与 Tekes 的 KeyboardFocusRoute 后，
仅为原 Send/Stop Content 配置已发布 TekesUI 的 `preserving_focus()`。
没有手动恢复焦点、复制输入/IME 状态机、修改宿主窗口或编辑 TekesUI。

该 SDK 属性保持 pointer/AX Press 时的原编辑 owner，仍允许显式 Focus。
本轮不声称已验证原生 Tab/快捷键逃逸路线或物理 IME。
新增回归在旧实现失败，修复后验证 Stop 激活、Send/Stop 切换、原编辑焦点及 marked baseline 保留，
组字期间 Send 不产生命令。第二公开 consumer 的 `verify_primary_keeps_editor` 使用同一公开组件和
SDK 事件独立验证，未依赖 demo 内部状态；该函数在回归中实际执行。

## 真实组件闭环

使用现有 presentation、原 Summary action、ChildPaneBinding、SidePanel、RunPort、Composer 和
原 metadata callbacks。CLI `--ui-child-run` 仅允许隔离的 Pi/DSH 离屏 profile，拒绝 Native/桌面组合。

- Pi 1.4.0 与 DSH 0.2.1-rc.2：从原 Summary 的真实 child pointer 打开独立 Run，核对直接 parent。
- 原编辑器合成 preedit 时 Send 被阻止；确认后只发送一次，回复必须属于本轮 input 的 completed turn。
- 发送异步派发，保持真实宿主的事件循环；观察包含本轮取消输入的已发布 Stop 帧，再按原主按钮取消。
  provider 在该专用 profile 中将受控响应保持到观察期限之后，必须由真实 harness cancel 终止；
  不用普通回复完成冒充取消，也不把 producer 的瞬间状态当作已呈现按钮。
- Stop 后不补焦点，直接在原编辑器输入暂存草稿；Summary 切换返回原 Composer IDs 和草稿。
- 重连到新 epoch 后两份输入各只有一个非 provisional 记录；父历史没有子输入，父及另一会话草稿不变。
- 原关闭按钮只关闭本地子 tab，旧 RunBinding 控制权失效；没有将关闭伪装为远端 discard。

最终同程序 DSH 在两个全新独立会话复核通过。较早 DSH 停止态瞬时检查失败及复跑记录保留，
不能据一次复跑宣称时序稳定；最终探针使用异步派发、已发布同帧检查和受控取消窗口。
Native 0.9.3 使用同程序另跑原 SideChat 的发送、切换草稿、重连和确认丢弃，结果通过。
Native 是显式 UI-only，不转移上一阶段有界历史/协议程序的证明；DSH 仍无 disposable SideChat 能力。

Task tab 仍按权威任务行解析 session，保持 task ID 作为 tab 身份；工具调用或 todo 本身不授予
独立 session pointer，本轮没有编造任务目标或另建任务状态机。实际 Task 来源组合仍待补验。

## 验证与范围

完整 UI workspace：618 通过、1 项既有忽略，含 demo、组件及第二公开 consumer；
workspace all-target Clippy、fmt、静态依赖边界及 Python 74 项通过。
根协议源码未修改，不把以前 root 310 项计为本轮重跑。
精确源码、reference hashes、最终程序和四份原始收据见[JSON](rust-agent-ui-primary-focus-child-runs-20261023.json)。
`.runtime-fixtures/real-child-run-ui-20261023/` 保留失败/成功日志、PNG、曾使用的 demo 程序与最终源码副本。
构建始终复用共享 target，没有新 target，没有签名/安装宿主或权限请求。

全部结果为同进程、真实受控 harness 加离屏/合成输入；`nativeInteraction`、`physicalIme`、
`readingAnchorAcceptance`、`crossProcess` 均为 false。当前完整交付仍未完成。
TekesUI 精确 pin 仍为 `50ec49f`；#48 ProcessContainer、#53 嵌套阅读、#25 富内容等原阻塞不豁免。
