# 系统提示处理后的原生续验

用户确认系统提示已处理后，复用现有 target 构建并冻结当前验收程序。
程序 SHA-256 `2bb325fe2e5c8b2c0ccb4111c2700e152aa69600d2d189fe5806a612648bcc1d`，
75,982,272 bytes；源码逐文件身份保存在 `.runtime-fixtures/native-handled-20261022/program.json`。

SideChat 菜单原生检查未通过：实际窗口被 CUA 观察到，原探针从 phase 14 推进至 20，要求终态 25。
前台轨迹显示启动约 22.17 秒后验收程序才获得前台；原计时自 Probe::new 起算，
25 秒期限在 phase 20 触发断言。随后前台为另一个 TekesUI 验收程序。
不能把期限失败归因为某项 SideChat 业务断言失败，也不能据此宣称功能通过。
LaunchServices 退出 0 仅表明等待关闭，包装层识别原程序 panic 后退出 1。

没有自动重启、重激活或放松期限；没有操作另一验收程序。
本次未继续分页、焦点及中文组字验收。日志及前台轨迹哈希见[机器记录](rust-agent-ui-native-handled-20261022.json)。
保留一份本轮必要程序，复用既有 target，没有删除缓存，实际释放空间 0。
