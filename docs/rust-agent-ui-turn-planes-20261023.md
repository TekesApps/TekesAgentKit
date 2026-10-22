# Activity/output 分区与 live 状态（2026-10-23）

对照当前 ClientKit TranscriptMetalActivity.isFrameNarration、remainingNodes 与 TranscriptMetalAppBridges，以及 Tekes 组合 ClientKit 设置的宿主。最新 SessionEventProjector 保持 assistant 的原语义；ClientSQLiteSessionStore 写 activity group 时 narration 为空，正文保留在第一类 timeline item。因此不为 Rust 猜造独立 reasoning/narration 类型或复制一份 group 正文。

## 公开状态与接入

新增纯只读 `TranscriptSection::planes` 和 owned `TranscriptTurnPlanes` / `TranscriptItemPlanes` / `TranscriptPlane`。reasoning 属于 Activity；只有已提交且明确 final 的 assistant 正文属于 Output；其余机器正文保持 Activity。非 final assistant 正文只有在已知同 response frame 的实际 call DTO 存在时标为 narration，不改 source kind。缺 frame provenance 的 legacy 分类返回 None；无 owner 的目录不参与关联，外部 scope、空 owned frame ID、错误 live owner 拒绝。

正常 Run 接入使用 `RunSnapshot::turn_planes(turn_id)`；`live_presentation_turn` 复用原 footnote 判定，已移动到共享 presentation。连接、历史新鲜度、选择、可用 phase、child 可写性和原 can-submit/cancel 共同构成展示资格，旧 running 字符串不会把历史 turn 提升为 owner。`latest_open_turn` 也供原低层 clock 验证复用。没有新增状态机、IO、mirror 或权限版本。

`TranscriptContent::turn_planes` 只读取已安装的 snapshot 和已发布 timing；第二 consumer 的 installed_turn_planes 直接转交该 API。只读 child 继续使用自己的 scope/publication 和 false live ownership，不构造父 Run 或可写 controller。分区 facts 不是发送、复制、历史读取或反馈权限。

## 当前原生组件行为

原 Transcript planner 新增 `TranscriptContentRowKey::ActivityStatus(turn_id)`，在 opening input 后、首个机器内容前显示 live Working。它从原 timing 的 live owner 产生；final candidate 不会提前让 live 状态变成 settled disclosure。结算/失去 owner 后退休节点，原消息与文本 owner 保留。

它使用 TekesUI 原 TextContent/UiCaption 与布局，只是被动状态，无自绘 spinner、焦点、IME 或滚动实现。宿主通过新增 `TranscriptContentLabels.activity_working` 提供 copy；完整结构字面量需要补该字段，推荐使用 Default update。通用被动图标仍等待 #47。

这不是已完成的 plane 容器布局：分区 facts 尚未物理拆分 OutputMessage 中的 activity/answer 子项，历史 activity disclosure、narration carrier/fallback handoff、完整富内容和最终 ClientKit 布局仍待完成。现有行和控件不复制、不丢源内容。正式 ProcessContainer 同/跨进程仍等待 #48；同进程 ContentHost 不代表该验收。

## 验证

最终根 workspace 285 项、UI workspace 590 项通过；UI 一项原有 #49 忽略。根/UI 全 targets strict Clippy、格式、依赖边界和固定 SDK 输入检查通过。新四项 plane 测试覆盖 final/非 final narration、保留 reasoning、live candidate、来源缺失/外部 scope/unowned map/空 frame。公开 Run API 与原组件 getter 在同一 publication 一致；原 live-owner 判定从组件移入 presentation 后，完整 UI 回归保持。

当前 UI 程序 SHA-256 `3e0eeb6a127cc496b08f5e8134a30e38026c82eb2c05c6e93a2e60db035870be`，搭配当前源码 headless 程序，在真实 Pi 1.3.0、DSH 0.2.1-rc.2、Native Kernel 0.9.3 上均通过三个独立 native profiles。provider 是受控本地合成 HTTP 服务，不是外部模型服务：

| profile | 三端验证 | 限制 |
| --- | --- | --- |
| 原生 stream | live Working 实际可见、结算后实际节点退休；原消息/文本 owner、AX 选择区、焦点保留；单次提交、A/B 草稿隔离、重连无重放 | 不代表历史分页或物理 IME |
| 原生 stream-reading | 追加期间原 item/local anchor、中文 marked text 与焦点、原 editor 保留；完整流式正文、会话切换与重连阅读恢复、单次提交 | synthetic preedit/wheel；不代表系统拼音 |
| 原生 public history page | 前插/向上阅读锚点、原 scope、独立 A/B 阅读、草稿与重连恢复；Pi 50→56、DSH 50→59、Native 50→56 个已加载项 | nativePagingControl=false，不计原生分页按钮交互；标记字段中未验的 false 保留 |

两条最终三端 driver 均退出 0；全部 nine profile 原始收据和字段保留于机器记录。mounted offscreen 另外保留各端 56 条消息的公开历史窗口扩展、无重复与新 epoch 恢复证据，completeRunPort/completeTranscript/richContent/keyedReadingAcceptance 仍为 false。运行 harness 子进程不是 ProcessContainer 跨进程 UI 验收。Native 使用明确 kernel-ui-only，未重跑完整 lifecycle 或 bounded-child 协议。

开发过程首次 Pi 探针读了较新的 binding snapshot、对应树尚未安装，错误地提前断言 Working 缺失。已改读原 RunPort.displayed_snapshot，所有状态/owner 断言保留，三端最终都通过。中间 Native 在窗口切回 Codex 后超时，该次不计通过；相同中间程序单独复跑通过，随后最终程序重新通过完整矩阵。旧程序/失败日志不覆盖、不移作最终收据。

本阶段没有新的系统拼音或物理键盘验收。先前首次按键 Commit 异常及同程序复跑通过仍按 [TekesUI #41](https://github.com/TekesApps/TekesUI/issues/41#issuecomment-6351744540) 跟踪，稳定 IME 验收未证明；不会以 synthetic preedit 替代。VoiceOver 未验。

8734185 的 CI 两项基础检查成功、两项 macOS UI 失败；已读取失败 job，仍是严格 bounded_detail_reading（#53）。公开 SDK main 未变，#48/#53 仍开放；PR #10 继续 draft，不合并或打 tag。完整目标与剩余布局见[交付状态](rust-agent-ui-delivery-status.md)。

复用 `/tmp/agentkit-layout-text-292-target`、关闭 incremental；新建 target 0，纯编译缓存删除/释放 0 字节。冻结程序使用硬链接；保留失败/成功程序、源码/WIP、固定 runtime/SDK 输入及日志/收据/截图。原脚本退出时清理其受控临时 harness 会话，未清理共享 target 或其他任务。精确路径与摘要见[机器记录](rust-agent-ui-turn-planes-20261023.json)。
