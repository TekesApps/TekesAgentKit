# Summary 默认行文字带与元数据布局

对照当前 ClientKit `SummaryDefaultRowView.fittingHeight/layout` 与 `TekesListMetrics.sidebar`：默认标题可换行；左右文字锚点为 `fillInset + Regular`（两份 Regular）；行高为标题高度加上下 Regular，并保留标准 body 行高。trailing 元数据相对标题第一行居中，不能相对整个多行标题居中。header 标题也使用同一横向锚点，自己的标准控件高度继续来自库。

Source 的标题使用 wrappingLabelWithString，trailing 使用 labelWithString；[Apple 对后者的公开契约](https://developer.apple.com/documentation/appkit/nstextfield/init(labelwithstring:))明确不换行。当前精确 TekesUI `09b4bab44b6dfa65487250ea816b610b9df59095` 已提供原文本、CompoundButton、Exact frame 与 label layout，可以直接消费，不修改 SDK。

## 组件与宿主 API

- 普通动作行保留原 `SummaryActionRow` 和 `button()` 检查接口，内部原 SDK CompoundButton 承载一个被动文字槽。按钮负责 backing、输入、capture、取消、AX Press；Summary 只布局自己的文字带。继承 SDK theme/foreground，不画按钮、字形或焦点。整行仍不成为 first responder。
- 静态行仍由原只读 TextFieldContent 管理选择/复制与受控发布，外包 Summary 专属布局容器。`row_id` / `passive_text_id` 对普通静态行继续返回原 field ID，没有把选择转存到 Kit。
- 新 `default_row_ids(section,row) -> SummaryDefaultRowIds { row, title }` 提供真实行带和标题槽。动作 title 是被动 TextContent；静态 title 是可选择 field。`usage_row_ids` 和 host custom rows 保持独立契约。本次不强行给宿主产品 Content 套默认行布局。
- 用量列保持现有 `SummaryUsageRowText` 格式化 seam。数值通过原 SDK `single_line(Clip)` 保留单行，完整模型/AX 字符串不被截断；宽度不足时由 SDK 裁剪可见字形。左右边距与标题带一致；metadata 的 y 来自库测量的 body 首行和原 caption 高度，保留带符号的居中量，不手工截字符串或估算字宽。
- header 仅修正 Summary 标题槽横向锚点和原 glyph 布局，原控件测量/高度/绘制与单行策略保留。Sidebar 共用 helper 的分组不改变。

原 `SummaryLabels::row` 字符串回调仍兼容。所有非用量的格式化结果在完整 preflight 中捕获一次，再用于原文字/动作；动作名为空会在改动图之前拒绝，满足 SDK CompoundButton 的可访问名称约束，不留半更新或提前退休自定义控件。格式化器应是当前输入的纯投影；没有新 Agent 状态机。

## 验收

第二公开 consumer 检查两种主题、四档字号、三种宽度的普通动作/静态长标题：真实换行、上下间距、同一横向锚点、原按钮/field与选择保留、无装饰标题重复AX、原动作和焦点策略。独立元数据场景检查长值仍为一行、原完整源文本和首行对齐。空动作名的预检与单次格式化有回归；原 ownership、child 状态、窗口与阅读测试继续运行。

UI workspace 474 tests、脚本31 tests、strict all-target Clippy、fmt、边界和实际 Cargo 八包241输入检查通过。完整474项覆盖最终业务布局；最后仅修正旧原生探针的8pt期望为两份 Regular，另经 strict Clippy/build 和全部相关原生模式验证。最终提交CI校验完整树；本轮没有重跑未修改的 headless workspace。

最终验收程序 `.runtime-fixtures/AgentKitSummaryRowsAccepted.app` 的 SHA256 为 `b666b8951cc287837ba48c7b3ef7a42e1317d223679d3fc5b9616942a9b33979`。同一程序通过默认行、label、child status、sections、workspace context、custom Content、reading 七个窗口模式；reading 验证分页20→44、Start/Latest、向上阅读、A/B草稿与重连时原焦点/marked baseline。系统简体拼音软件输入确认组字→确认→单次发送，输入源保持；不是实体键盘或候选窗验收。

中间程序及失败记录均保留：两次旧 sections 断言仍使用8pt；第二次因修改脚本未命中而运行同一程序。最终直接修改源码期望，并在整个命令开头启用失败即停。没有删除失败记录、改变 SDK 或放宽原控件/焦点/组字断言。

原生入口仅用于 owned SEP fixture：

```sh
cargo +1.95.0 run --manifest-path ui/Cargo.toml --locked --offline \
  -p tekes-agent-ui-demo -- --components --transcript-content --summary-row-layout-selftest
```

复用现有 target。可选 `AGENT_SUMMARY_ROW_CAPTURE` 必须是新的绝对 PNG 路径。探针使用真实窗口、原角色文本/控件、公开 SEP inventory 更新和共享 presentation，验证静态选择、原动作、中文 marked baseline、resize、字号变化及 metadata；不绕过 IME 屏障或写 scroll offset。截图是本程序布局证据，不是完整 Tekes 配对视觉验收。

## 未完成范围

这补的是当前默认行的文字带与用量 metadata，不代表所有 SummaryDefaultItem 已交付。普通任务/子代理仍有实验状态文字回退；leading 图标、状态附件（TekesUI #47）、非用量独立 trailing metadata/tone、产品级仓库组合、header hover/divider、其它 Views 与真实三端完整组合仍开放。静态 field 本体可选择，整行留白点击的 Source 行为尚未等价：SDK `set_padding_focus` 当前拒绝只读目标，已在 TekesUI #30 补充可直接开发的只读消费契约和验收；不自写命中/焦点/选区引擎。没有用 Unicode 或自绘点冒充库组件。

精确程序/源码/测试/原生收据见同名 JSON 与 `.runtime-fixtures/summary-row-layout-20261022/`。物理键盘、候选窗、VoiceOver与完整 paired layout 不由组件、合成预编辑或程序 resize 证明。PR #10保持实验草稿，整体目标继续进行，不据此合并或打稳定tag。
