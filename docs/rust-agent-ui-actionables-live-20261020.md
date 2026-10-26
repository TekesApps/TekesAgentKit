# 三端公开问答卡片闭环

本轮消费既有 ActionableCard、Composer、RunBinding 和 presentation，在实际 Native / Pi / DSH 上验证一次问答，不增加业务状态机。TekesUI pin 保持 `431093786de47b48cd566d0d9c0eafa088b10d68`，没有修改 Kernel、ClientKit 或 TekesUI。

## 验证路径

1. 公共 Composer 的 Send 回调触发真实 harness 提问，公共 ActionableCard 产生选项回调。
2. 捕获原 scope、epoch、canonical actionable 后，先切换 B，再回传 A；同一 request 第二次 respond 必须失败，B 的独立草稿保留。
3. 等待共享 actionable 移除、运行回到 Idle、成功答案进入真实历史，再通过公共 Send 发送后续输入。
4. 重连等待新连接 epoch、ready 和 journal_ready；重新读取历史，核对后续输入稳定 ID 恰好一条、答案仍在、旧 request 失效、B 草稿不变。

Native 经公开 session.create 的 execution.allowed_tools 在隔离会话启用 ask_user_questions；合成 Responses provider 调用真实工具，成功 tool/result 中保留 continue-proof。Pi 临时扩展通过公开 ctx.ui.select 提问，appendEntry 持久化实际选择 yes；没有注入虚构 settled 事件。DSH 合成 chat provider 调用真实 ask_user_question，成功 tool/result 保留 continue-proof。三端共同调用已存在的共享回答契约。

## 复现

在现有 real-session-endpoints 命令上增加 `--ui-components --ui-actionables`，并传入构建好的 `--ui-demo`。可以分别配置 --kernel-root / --kernel-target-dir、--pi-root、--dsh-root；各 runtime root 必须是明确、已验证的本地版本。Native 测试会额外创建仅启用提问工具的临时会话。脚本使用临时 HOME / dataRoot 和本地合成 provider，不使用真实凭证或付费服务。

实现位于 ui/demo/src/actionable_proof.rs；脚本强制核对三个 harness 的机器回执。第二公开 consumer 的卡片 API 验证继续由 UI workspace tests 覆盖。

## 证据边界

这是实际 harness、共享 runtime、公共组件回调及离屏 GPU 挂载验证；不是物理鼠标/键盘、真实系统中文组字或 VoiceOver 验收。此前本地 fixture 的原生卡片操作证据独立保留。三端本轮只验证单问题单选，不代表真实审批、多选、自由文本、secret 或 MCP forms 已支持。正式 Transcript/RunView、阅读与分页锚点仍待公共 TekesUI #22/#23；2026-10-20 查询两项仍为 OPEN，不改变 pin。

机器回执与确切二进制来源见 rust-agent-ui-actionables-live-20261020.json。本轮不合并、不打 tag，PR #10 保持 draft，完整交付目标保持不变。

本地 UI workspace 全部 171 tests（含第二公开 consumer）、工具 18 tests、UI strict Clippy、fmt、边界检查及实际解析的 8 个 TekesUI 包/188 文件校验通过。根工作区业务实现本轮未改动；提交前已有 PR CI 全绿，新提交仍需远端 CI。
