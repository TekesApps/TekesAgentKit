# 历史输入入口与宿主回调（实验 API）

对照当前 ClientKit `TranscriptTurnFootnote.rawInputNodes`、
`ThreadTranscriptPresenter.onActivateAction` 和普通 thread snapshot：
输入脚注按 **信息 → 编辑/回退 → 复制** 排列，整体靠右；普通 thread 使用 Edit，
Rewind 需要显式配置。动作把原回合与原输入交给宿主 hook。
Tekes host 的 `ClientAgentHostAdapters` 只读作为边界参考。

AgentKit 的已发布 tag 基线是 v0.3.8，当前 UI 使用本分支同仓库契约/投影的开发路径，
包括尚未发布的 TranscriptDocument 等增量。本入口复用已发布 capability 字段，
但整个 UI 尚不能只依赖 v0.3.8 源码构建；共享增量需随本 PR 一并 review/tag。

## 接入

`InputMessage`、`TranscriptContent` 和 `RunPort` 默认隐藏入口。
只有实际端点 `can_edit_historical_input` 为 true，且宿主注册了编辑流程时，才配置：

```rust
let control = (handler_registered && binding.historical_input_available())
    .then(|| HistoricalInputControl {
        action: HistoricalInputAction::Edit,
        label: "编辑输入".into(),
    });
run.configure_historical_input(control)?;
// 随后通过既有 update / reading transaction / render 发布该配置。
```

在原 ContentHost 事件队列排空后、更新来源前，单独取这个显式回调通道：

```rust
for intent in run.take_historical_input_intents(&actions) {
    original_binding.dispatch_historical_input(&intent, |source| {
        open_host_editor(source.scope(), source.turn_id(), source.input(), source.action());
    })?;
}
```

`HistoricalInputIntent` 捕获原 scope、message key、完整 TranscriptItem（含附件与来源）、
回合、publication、epoch 和 Edit/Rewind。clone 共享一次 callback admission。
绑定重新核对实际能力、当前 runtime epoch、指针/selection、公开历史中的原输入和 recorded 状态。
过期、其他绑定、伪造内容、待同步历史及重复 claim 均拒绝。

这个 hook 用于打开宿主编辑流程，不改写历史、不替换 Composer 草稿、不发送 prompt，
没有新增 endpoint RPC。后续真正修改历史需要独立公开契约与当时的权限检查；宿主仍需验证
mount/navigation adoption。不要把回调的来源查询当作异步修改授权。

原 RunPort 在 Composer/反馈编辑器组字、pane 隐藏或 entries gate 关闭时不交付此动作。
独立 InputMessage/TranscriptContent 的宿主也必须遵守原生组字与 pane 屏障。
配置改变使已准备的帧失效；反复注销/注册不能复用旧 prepared plan。
只读/不可用出版物不挂历史编辑按钮，pending 输入没有该入口。

## 基础组件与当前能力

按钮、单行省略、焦点、文本输入和位置计算均消费固定公开 TekesUI
`50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。原信息、复制、正文和另一 Composer
不因新增动作而重建。双动作文本按钮各限制在行宽的 40% 内，信息使用剩余行宽；
这是缺少原图标入口时的 SDK 文本按钮路径，图标视觉对齐仍等待 TekesUI #47。

**当前 Native/Pi/DSH 适配器均未声明历史编辑能力，因此正常 demo 不显示这个入口。**
能力为 true 的测试使用明确的本地 authority fixture；不把 fixture 当作真实端点能力。
“Sent as goal”等其他输入 footnote 标记不在此次实现中；整体 UI 仍未达到完整 ClientKit 对齐。

## 验证范围

- UI workspace 全量：526 passed、1 个既有 #49 ignored。之后补强当前 runtime epoch/
  canonical source、namespace 预留和配置幂等检查，最终四项定向回归通过。
- 独立公开 consumer：4 主题、3 字号、770→320→180→770 列宽，检查同一右对齐带、
  完整 source、原控件实例、另一 Composer 的焦点与完整组字 baseline。
- 宿主/绑定：真实 capability 拒绝、原输入回调、duplicate/foreign/forged/old-epoch 拒绝，
  无草稿替换/提交；RunPort 组字、隐藏、只读及 prepared-policy ABA 回归。
- 最终严格 all-target Clippy、demo build、fmt、静态 UI 边界和实际 8 包/243 文件精确依赖核对通过。

这是组件、宿主适配和第二公开 consumer 验证，没有新增原生窗口验收。
之前 `2696969…` 程序的[三端补验](rust-agent-ui-native-resume-20261022.md)保留其原范围，
不转移为本次新入口的原生证明；失焦原因和未完成的流式阅读仍待处理。
ProcessContainer 跨进程仍等待 #48。没有修改 TekesUI、Kernel 或 Swift。

[机器收据](rust-agent-ui-historical-input-20261022.json)记录最终 Rust 源码、只读参照和日志哈希。
本地证据位于 `.runtime-fixtures/historical-input-hook-20261022/`。
复用共享 target，没有新增独立缓存；现用构建依赖和验收证据保留，本阶段未执行删除。
