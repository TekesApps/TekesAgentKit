# RunView / Composer context 拒绝更新时保留原帧

阅读事务联调发现，Composer 原先先同步受控正文，再执行 context 卡片的 revision/capacity 检查。如果 context 拒绝该帧，RunView 返回 Err，但编辑器已安装新草稿。新增公开 API 集成回归将一个已挂载 context 显式推进到较新 revision，再给 RunView 较旧的父帧，实际复现正文由“原草稿”变为“不能部分安装的新草稿”。

修复把现有 Agent context 检查提取为共享无副作用检查，在 local edit 收集、目的地替换和正文同步前调用；context 自身 update 复用同一检查。没有复制 TekesUI 编辑器校验。新增测试确认拒绝后原正文、TextEditBaseline、external value、Transcript 行均不变，随后显式更高版本可恢复；原 editor 实例继续使用。

## 验证

- 默认固定 TekesUI 下完整 UI workspace 289 项（包括第二公开 API consumer）、严格 Clippy、fmt、依赖边界和实际解析八包199输入检查通过。root 协议代码未改，本轮未重复全部166 headless 测试或三端真实 transport 探针。
- 原生 fixture 的 `--components --context-selftest` 通过；随后使用**同一二进制**运行 `--components --transcript-content --context-selftest`，正式 RunView/Transcript 挂载路径也通过。检查真实窗口控件、合成 marked text 屏障、发送前移除、共享编译、单次 admission、回传和 A/B 内容草稿隔离。截图已查看。这是 fixture 原生 context 专项；不是新回归拒绝场景的原生注入、物理 IME、真实 SEP 阅读或完整视觉对齐。
- 独立449文件候选回放保留原三项几何/状态验证，新增一项 #26 精确复现，共4项及严格 Clippy 通过。第四项明确验证**当前缺口**，不是 RunView 原子阅读成功验收。

## TekesUI #26 的精确冲突

[Issue #26](https://github.com/TekesApps/TekesUI/issues/26)要求受控文本的无副作用预检/prepare，并复用现有库内校验。

公开探针 `issue26_editor_composition_conflict_does_not_reject_the_reading_version` 挂载实际 Composer 和 Transcript；先令 editor.input.preedit 生效，捕获当前 baseline，再提交同 identity 的更高 DraftRevision，payload 是另一份权威草稿。reading transaction 回调内调用原 surface.apply_external_value，得到 `Conflict(Composition)`，编辑器基线/正文和 Transcript 未变；外层 reading API 却返回成功且 reading document version 已推进。它按现有 callback 契约工作，不能用于拒绝整帧。

消费侧继续只在候选事务里提交预检过的 Transcript。完整 RunView 需要可组合的预检/提交契约后再接，不克隆原生编辑器、不复制通用判定，也不通过刷新当前 token 覆盖旧请求。精确复现保留在 [`scripts/probes/agentkit-reading-candidate.rs`](../scripts/probes/agentkit-reading-candidate.rs)，使用已有隔离 runner 可重跑。

## 参照与剩余工作

按用户更新的目标，Layout/显示逻辑参照[当前 ClientKit 对应 Views](clientkit-view-layout-review-20261020.md)，基础视觉继承 TekesUI。已记录正式 RunView 叠层/列布局及完整回合 rail 等差异，不能以状态文本和测试宿主布局替代它们。上游发送方的 bd3 原生阅读报告不提升 AgentKit 的接收方验收范围。默认 pin 未变，UI 仍实验状态，完整阅读/富内容/并排布局、最终 review/合并/tag 尚未完成。

[JSON 收据](rust-agent-ui-context-preflight-20261020.json)保存输入与日志哈希。早期新增回归曾因测试代码误用 Content.text_surface、Arc snapshot 而不能编译；修正测试后先重现真实失败，再修实现；编译失败和断言失败日志均保留，不作为通过证据。
