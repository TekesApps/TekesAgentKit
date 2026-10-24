# 工具详情：独立分区与复制

OutputMessage 和 ToolActivity 默认把共享 `presentation_details` 的 Input、Output、
实际 Call view / Result view 分别挂载为原 TekesUI 只读文本字段。
标题、复制按钮、横纵向组合分别消费原 TextContent、ButtonContent、StackContent。
机器内容保持左对齐；字体、圆角、颜色和间距使用 SDK 默认角色与 tokens。
没有复制字形、选区、绘制、滚轮路由或文本服务实现。

同一 scope/tool/分区的原字段在追加、结果结算及重连时保留。copy_text 与显示 text 分开：
复制整区保留原尾部 LF，显示按共享规则至多去掉一个末尾 LF。
原文或连接 epoch 变化会分配新的复制按钮 ID；可选视图删除后再出现也使用新 ID。
折叠期间不暴露复制授权。分区只输出本地剪贴板 intent，不授予任何 endpoint 操作。

原 TranscriptContent 汇总分区复制，RunPort 在组字屏障前仅接受原本地复制通道；
原 SDK 保焦点按钮负责指针/AX 行为。收到复制 intent 后，宿主沿用既有 `claim()` 一次写入，
使用捕获的 scope/source/text，不按当前选中会话重新查询。

## 开发 API 变更

- `OutputMessage::tool_detail_sections()`、`ToolActivity::tool_detail_sections()` 返回分区 body/header/copy 的原 ID。
  `OutputContentIds::body` 对已解析的结构化工具是容器 ID，不能再当成单个 TextSurface。
  未解析工具和普通正文仍有自己的原文本字段；解析状态改变时退役旧 body ID。
- 两者均提供 `copy_action_ids()` / `take_copy_intents()`；TranscriptContent、RunPort、只读宿主沿用现有复制通道。
- `TranscriptCopySource::ToolDetail { tool_id, turn_id, section }` 标识复制来源；`message()` 为 None。
  对该 enum 的穷举 match 需要补新分支。
- `OutputMessageLabels::tool_sections` 是显式的纯显示回调，默认消费共享分区。
  自定义进度由 producer 明确提供，不从 opaque meta 猜测。`text` 增加分区标题/复制等文案键。
  若宿主需要保留旧 `tool` 回调的单字段正文，显式设置 `tool_sections: None`。
  完整 struct literal 需要新字段，推荐用 `..Default::default()` 再覆盖所需回调。
- 分区顺序在挂载前校验，重复/非法分区拒绝；同 publication 的不同事实和旧 publication/epoch 不可覆盖当前状态。

这些 API 仍属于 0.0.0 / publish=false 的实验 UI workspace，不属于已发布 AgentKit v0.3.8。

## 证据与未完成项

本轮普通 UI 工作区 **559 passed、1 个已有 #49 ignore**；严格 Clippy、格式、精确 SDK 输入和 demo 构建通过。
新增独立 consumer 验证分区顺序/宽度/主题、各区所有者、原文复制、旧按钮失效与视图再次回传、
Composer 组字与指针复制隔离、非法 callback 和旧单字段显式选择。
原 RunPort 检查验证组字期间只派发原分区复制，不产生发送；既有展开/追加/重排/分页阅读回归继续通过。
旧测试曾在诊断头部选 0..2；改为选择结果正文后，SDK Preserve 随前插的 `updated ` 保留原 `re` 于 8..10，
未加入消费方选区补偿。

标准分区仍处于文档流。显式限高实验对每区消费原 SDK 视口，严格嵌套阅读检查仍退出 1：
父容器追加后偏移 634，详情不可见；[#53](https://github.com/TekesApps/TekesUI/issues/53) 未解决，不忽略该门禁。
SDK 已有被动 SeparatorView，但通用 Content 接入口缺失，已提 [#54](https://github.com/TekesApps/TekesUI/issues/54)；
当前用 SDK 默认间距分隔，不能视为完整 divider 排版。
富代码/卡片与原 code-header 样式仍待 #25，紧凑图标/活动附件待 #47，ProcessContainer 待 #48。
没有本轮新增的原生窗口、真实 harness、系统输入法、VoiceOver 或跨进程验收。

源码及证据哈希见[机器记录](rust-agent-ui-tool-section-controls-20261023.json)。
复用原 target；未复制新的验收程序、未新建 target、未删除缓存或证据，释放空间 0。
