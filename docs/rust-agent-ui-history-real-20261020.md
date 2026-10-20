# 三端分页与重连验证

在公开 HistoryPageRequest 上补充真实 Native、Pi、DSH 的隔离联调。每端新建独立会话，通过公开 SEP 连续提交 28 轮，等各轮精确输入对应的最终回复后，再连接离屏组件宿主。模型服务使用本地合成 provider，不发起付费调用。

A 捕获分页请求，切到保留中文草稿的 B 后完成 A 的分页。验证原 scope 的 Loading / Loaded 回执、冻结边界、既有消息身份与正文保留、无重复、B 草稿和当前选择不变。重连后等新 epoch 的权威历史 ready，再核对已加载历史完整保留，旧读取回执失效。完成这些检查后，测试宿主才显式导航回 A，输出恢复的数据投影。

此过程复现并修复了 Pi 的历史丢失：NativeConversation 初始 follow 固定请求 50 条，即使用户已分页到 100 条，重连仍缩回 50 条。runtime 现在依据原投影的已加载窗口大小重新向 endpoint 请求；新快照仍覆盖旧快照，不把缓存拼成权威历史，不修改 Kernel、Pi 或 DSH。新增确定性测试要求 follow 请求序列为 `[50, 100]`；旧实现得到 `[50, 50]`，真实 Pi 也在消息身份保留检查失败。修复后两者通过。

测试工具的 waitSettled 增加可选 inputText：只接受最新精确用户输入所在 turn 的最终非临时回复，避免在被裁剪的历史窗口里以绝对轮数判定完成。两项单测排除旧回复、流式回复、其他 turn 和较早的相同输入。

本地根工作区 127 tests、UI 工作区 178 tests（含第二公开 consumer）、工具 18 tests、两套 strict Clippy、fmt、依赖边界及实际 Cargo 输入校验通过。三端均通过分页、重连和离屏宿主检查；[机器收据](rust-agent-ui-history-real-20261020.json) 保存输入哈希与各端结果。TekesUI 继续固定公开提交 `431093786de47b48cd566d0d9c0eafa088b10d68`。

复现入口为 `scripts/test-real-session-endpoints.py`，提供对应 `--kernel-root` / `--pi-root` / `--dsh-root`、已构建的 `--ui-demo`，加 `--ui-components --ui-history-pages`。这是数据、公开绑定和离屏 GPU 证据；正式 Transcript / RunView、原生中文 IME、滚动阅读锚点与混合文档前插仍未验收。

布局与样式契约不变：机器内容及 footnote 靠左；用户圆角输入气泡靠右，用户 footnote 在气泡外靠右。视觉使用 TekesUI 默认样式、主题和语义 tokens，不新增 AgentKit 自有颜色、字体或圆角表。UI 库的整体调整通过显式升级精确依赖传递，通用能力缺口仍交由 TekesUI issue 处理。
