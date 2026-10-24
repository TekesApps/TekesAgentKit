# 纯文本用户消息行与实验 UI 输入升级

`InputMessage` 是首个可独立挂载的 Transcript 内容行，不是完整 Transcript / RunView。它从既有 `TranscriptInput` 读取整条用户正文与 delivery，沿用 scope、publication、connection epoch；仅显式 submission receipt 允许 pending 到 NativeEntry 的身份迁移。旧来源、旧 publication 与同版冲突拒绝更新。

排版归 AgentKit：正文一个气泡靠右，footnote 独立放在气泡下方靠右。样式归 TekesUI：默认 Surface、只读 Content 正文、UiCaption 和语义间距；颜色、圆角、文字度量、选区、焦点、文本服务均消费公开组件。宿主渲染时传入主题，不维护第二套样式表。短输入按正文收窄，长中文在可用宽度内重排。

当前只支持纯文本；附件、推理和非文本块在修改前显式报错，避免缺失内容却伪装完整消息。机器正文、工具/推理、两类完整 footnote 动作、混合文档阅读和 RunView 后续继续实现。混合锚点仍受 TekesUI #22 约束。

实验 UI workspace 已固定到公开 `8cf0840790334ba224afc0595897c74231c712e6`；实际八包199输入 SHA 为 `f0af230a1efe97334b90eb193f77ca0d30dec43ef09e487ad6948c38a938257f`，来自此前独立完成的 #23 后继评估。原431清单逐字节保存在 `ui/baselines/`，非 UI lock 包与依赖边保持原值；不是 AgentKit UI release。

验证：macOS CoreText 测试覆盖浅深色、短正文收窄、长中文窄窗换行、外置右侧 caption、只读编辑防护与来源/身份 fencing；第二公共 consumer 直接从文档创建并挂载该行。测试宿主 `--components --input-message-proof --offscreen /tmp/input-row.png` 对真实 fixture snapshot 挂载一行，离屏结果 bubble=146、row=532，几何断言通过。自动文本服务与离屏测试不代表物理 IME、VoiceOver 或真实三端原生交互验收。

本轮完整 UI workspace 187 项测试通过，包含第二公共 consumer；严格 Clippy、fmt、所有权边界、199文件实际解析输入校验通过。headless 源码没有变化，三端协议不在本轮重新验收范围。


后继扩展见[混合内容树](rust-agent-ui-transcript-tree-20261020.md)：当前 InputMessage 接收附件及未支持内容的原始信息详情，按现有 App 的附件 → 正文气泡 → 外置 caption 排列并统一靠右；纯附件隐藏空正文目标。相同附件按出现次数对账，避免 canonical 与 aggregate 镜像重复展示，也不会吞掉两份相同附件。以上纯文本收据及 187 项测试数量仍指原始提交。
