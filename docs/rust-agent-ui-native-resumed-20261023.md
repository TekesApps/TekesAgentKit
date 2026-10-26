# 原生续验：SideChat、阅读与系统拼音

系统提示处理后，本轮原生程序取得以下收据：

- SideChat 菜单到达 phase 25：两个实际 scratch pointer、同名标签和当前成员选中、编辑器保留、关闭后删除 scratch、主草稿保留及无提交均通过。
- 分页/阅读到达 phase 16：由原分页控件从 20 条加载至 44 条；前插锚点、向上阅读、会话 A/B 阅读恢复、重连恢复、同一 editor、原焦点/组字基线保留均通过。该用例使用本地 SEP fixture、合成滚轮及测试 Preedit。
- 系统简体拼音通过：使用 CUA 对绑定窗口发送 `n i h a o`，AX 与原生日志均观察到 `ni hao` marked text；原发送/会话切换控件在组字期被拦截；COMMIT_READY 后空格确认成“你好”，原发送只产生一次提交。输入源保持 `com.apple.inputmethod.SCIM.ITABC`，没有粘贴或替换字段，也没有由驱动注入 Preedit。

本轮先有一次启动参数错误：少传 `--panel-navigation-toolbar`，启动断言退出。日志保留，不计为功能失败或通过。随后观察工具在该程序退出后启动过无参数演示窗口，已退出；成功收据来自补齐参数的独立启动，未放宽原探针期限。

程序 SHA-256 `7c531ae04db52752dcb533648f702dda362a687e60eb1a5448e27324f5ab03f4`，76,580,880 bytes。
这是 `5a8e9e0` 上连续工具分组 WIP 的一次冻结构建；逐个 Rust 输入哈希在
`.runtime-fixtures/tool-run-20261023/native-program.json`。随后补充的 URL 解码及分组测试不属于这份程序，不能把本收据转移到后续源码。
原生程序仅保留一份供复验，复用既有 target，无新增独立 target，无缓存删除，释放空间 0。

这些结果属于本地协议演示，不代表真实 Native/Pi/DSH、实体键盘、候选窗完整体验、VoiceOver、ProcessContainer 同/跨进程或完整 ClientKit 布局验收。TekesUI #53 严格嵌套阅读仍失败，PR #10 仍为草稿，不合并、不打 tag。

日志、前台轨迹、成功收据和程序身份的哈希见[机器记录](rust-agent-ui-native-resumed-20261023.json)。
