# 思考载体与详情排版（2026-10-23）

对照当前 ClientKit TranscriptMetalActivity 的 reasoningCarrier、reasoningRows、hasReasoning，以及 Tekes 设置宿主只组合 ClientKit 配置的方式。本阶段仅修改 AgentKit，基础控件与精确 TekesUI pin 不变。

原 Run/Pane 来源帧工厂现在按公开 response frame 选择思考载体：有非空白思考的消息参与，完整文本字符簇数量较大者胜出，相等时用原 ID 作稳定 tie。独立 frame 不互相压制。None 的最新被动提示仍由原生命周期策略决定；carrier 仅限制重复 disclosure。选取依赖的全部消息文本纳入 source fingerprint，显示修订不能携带同 publication 下偷偷变化的兄弟消息来源。

`TranscriptItem::display_reasoning` 在存在已证明 block 顺序时从 Reasoning blocks 合成规范正文；只有 legacy 无 block 来源才读 aggregate。它保留源字符和空白，空白判断使用 Unicode whitespace。受控 OutputMessage 将同一消息的思考块合成一个稳定 owner 的详情，放在内容之前，显示时修剪首尾空白；文本/工具/附件的相对顺序不变。详情内复制对应显示文本；原 DTO 和整消息/turn 复制仍使用原 source。

隐藏的或纯空白的已证明 reasoning-only 消息不产生 Transcript 行，也不留下空白间距/虚假的工具分组边界。未知、legacy、错误、工具及附件仍保留，不按相似正文猜关联。显示源切换后，原 prepared plan、reading token、namespace 和迟到 action 门禁仍生效；已发布 turn target 只从已发布显示输入计算，不提前使用尚未发布的偏好。

范围：这不是完整 narration/carrier 或 ClientKit 时间线布局验收。当前归一化 kind 尚未表达 ClientKit 独立 reasoning/narration 记录的 kind 优先级，因此不把 provisional 猜成该 kind。独立 reasoning kind 接入、narration carrier/fallback handoff、不同来源的平面展示顺序、富文本和 ProcessContainer 仍待完成。底层 `TranscriptThinkingFrame::read` 仅接收生命周期 facts；宿主正常接入应消费 Run/Pane 工厂或整帧组件，不自行按正文选载体。

## 验证与范围

根 workspace 278 项、完整 UI workspace 588 项通过；UI 仅一项已有 #49 忽略。根/UI workspace 全 targets strict Clippy、格式、依赖边界、固定 8 包/243 输入检查通过。新用例覆盖字符簇计数与单 frame 载体、独立 frame、源 fingerprint、已证明 blocks 优先级、空白、合成详情前置、流式新增块保持原 editor/选择区/焦点、以及父 Transcript 不为纯空白建立阅读行。原 SEP→RunBinding→第二 consumer 用例改为校验修剪后的显示文本，另行断言源 reasoning 完整保留；原末尾换行断言失败及误用测试 API 的编译日志保留。

新程序 SHA-256 `6a3f043eee05a7d057019c5a10b4ce70af4c239ba3690b4ad60564238ca5f370`：原生窗口的本地 fixture 分页 20→44、前插/向上阅读锚点、焦点/preedit、A/B 草稿、会话与重连恢复通过；系统拼音 `ni hao`→`你好`、发送/导航组字门禁和单次提交通过。按软件定向按键记录，不代表物理键盘、候选窗、真实 harness 或跨进程验收。

已读取 5fc496a 的终态 CI 及失败 job：两项基础检查成功，两项 macOS UI 失败仍在 unchanged strict bounded_detail_reading（#53）。新提交以 PR Checks 为准；不合并或打 tag。#48 的正式进程交付仍未具备。

本阶段未新建 target，复用共享 target 并关闭 incremental；验收程序硬链接保留，没有第二份程序内容分配。删除/释放 0 字节，原依赖、WIP、日志和验收程序保留。准确输入与证据摘要见[机器记录](rust-agent-ui-reasoning-carrier-20261023.json)。
