# #15 / #16 P0 消费侧收口（2026-10-18）

状态：部分完成；不是可关闭结论。无提交、推送、tag、Issue 关闭或性能测量。

## 根因与实现边界

#15：上游 WindowWake 已存在；消费侧仍持续请求帧发现业务更新。宿主现在订阅既有 RunController watch，最新 snapshot 发布后投递 Event::Wake，请求重绘；异步操作先入结果队列再 wake，覆盖没有 snapshot 变化的错误结果。关闭时终止订阅，代理不保活窗口。有限控件动画继续按原 request_animation_frame 开停；只有显式 native-selftest 有两个有限完成/失败期限通知，无业务轮询定时器。

#16：上游版本化 ReadingController 已存在；消费侧直接替换 transcript 字符串，丢失稳定消息身份。AgentKit 组件层按 TranscriptItem.id 转为保留 UTF-16 区间，由上游计算恢复位置；相邻连续保留区间合并，流式内容只映射双方字素边界内公共前缀，重复 ID 不映射，删除/变化段按公共契约失效与 fallback。每 destination 仍保留同一控件及 reading 实例，UI 只同步 controller 的最新 snapshot；不应用异步回包自行算出的像素补偿。

共享修复复用 TekesUI 已有 wake.rs / reading.rs / TextSurface；本轮未改 TekesUI 公共实现、token 或设计源。适用本试点的所有 destination、流式/取消/重连/分页更新。合理例外：Composer 是可编辑面，不套用只读历史更新；其受控编辑/IME 缺口仍归 #17–#20。#21 动作计数出口不扩大迁移。对照 ClientKit TranscriptScrollCheckpoint 与 ThreadTranscriptPresenter+Scroll 的身份/阅读意图，以及 Tekes ClientAgentHostAdapters 的宿主边界；不修改 Swift 或 Kernel。

## 依赖身份与保护

显式消费全部最新 WIP，ui/tekesui-source.json 的开发快照 SHA-256 为 83ea59f05b430210bc60fa098e150d133ce8e4f9dae35220337a9b7e802b7359。旧清单及旧 .deps 原样移动至 /tmp/tekesui-p0-closeout-20261018，未覆盖。仍是开发快照，不能代替可公开获取的不可变 commit/tag。三个 checkout 逐文件保护基线与本轮 source overlay 随证据保存；ClientKit 全部 WIP 保持原样。

## 分层证据

- 公共 API：复用已冻结 wake-contract / prepend-reading 批次；本轮没有共享代码变化，未将旧证据改写为新验收。
- 真实接入：UI workspace 11 项通过（组件映射 2、第二 consumer 1、通知 2、真实 CoreText 阅读 2、Run 流程 4）；strict Clippy 与 demo build 通过。真实 Earlier 连续两页 <=1pt，选区/Composer 焦点保留，A→B→A 保留阅读位置。通知测试经真实 SessionRuntime / RunController 验证流式、取消、断连，无轮询帧；确定性覆盖通知处理期间再发布以及目标停止。
- 默认 headless：76 项通过；UI 仍为独立 experimental workspace，Rust 所有权/依赖边界脚本通过。
- GPU：当前 AgentKit demo 用实际 Renderer/CoreText 离屏绘制，图已查看。基础连接/正文/Composer/动作可见，无裁切发现；仅代表当前 light/1000×760 初始状态，不证明分页体验或全主题验收。
- Gold7：沿用 JSON 字体/样式及现有控件；行为侧用真实分页、位置/选区/焦点、显式 follow 回归。旧 Gallery 四主题/两次分页 GPU 与原生证据仍独立保留。AgentKit 的原生过渡、输入和最终体验尚未验收，不由离屏图代替。
- 原生 AgentKit 窗口：未启动/未操作；现有桌面授权仅 Gallery。多目标原生 wake 隔离组合仍未完成。物理中文 IME、VoiceOver 未测；性能按用户要求暂不测。

## 关闭前还剩

1. 在明确授权的 AgentKit demo 原生窗口实测无 frame pump 的流式/取消/连接变化与实际 Earlier 阅读连续性，记录事件/帧、焦点、截图及二进制身份。
2. #15 补多窗口/宿主目标隔离证据；当前单窗口公开 App 路径不凭空扩成新框架，弱目标关闭与状态隔离已有证据不等于现场组合完成。
3. 正式公开不可变 pin；受用户禁止提交/推送/tag 约束，继续标阻塞。两项 Issue 保持开放。

证据：TekesUI design/reviews/p0-consumer-closeout-20261018/receipt.json。
