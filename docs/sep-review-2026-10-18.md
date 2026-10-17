# 三个 SEP 对照复查

日期：2026-10-18。参考当前 ClientKit HEAD `c289bc9d33edcd83a782ac17927a8c65a027eccd`；已逐文件核对源码基准中的 SEP/契约哈希，无漂移。ClientKit Composer 的本地 WIP 未修改。所有修复只在 Rust AgentKit，无 UI 或 Swift 代码。

## 发现及修复

| 级别 | 问题 | 修复和证据 |
|---|---|---|
| P1 | Pi 指针文件丢失可能被 native RPC 当作新会话，原路径被替换也未重新核对身份 | 记录 materialized/native session ID；启动前校验文件存在与路径，RPC 握手核对 sessionFile/sessionId。旧注册表按已有 receipt/parent/元数据保守识别。真实 runtime 验证已采用文件被另一会话替换、随后删除均失败，删除后没有新建文件；单测拒绝 symlink 重定向。 |
| P1 | 重连时一个无法打开的 pointer，或持续损坏的会话流，可能耗尽整个 endpoint 的恢复预算 | 失败限定在该 session 的 journal_error，其他会话和主连接继续工作；显式 open 可重试，打开失败及时返回错误。回归验证丢失 pointer 与耗尽单会话流预算均不会重连健康会话。物理连接故障仍由全局流触发重连。 |
| P1 | Pi atSeq 按当前 ancestry 序号取值，与 ClientKit 完整 get_entries 序号不一致，可能切错分支 | 按完整原生顺序定位，核对活动分支及 tekes.run.v1 settled 边界；用公开 fork-at API 包含该精确 entry。显式 entryId 保留原生精确接口。单测涵盖旁支、未完成边界；真实 runtime 验证 settled marker 被保留而后续输入未继承。 |
| P2 | Native/Pi close 使用 watch.send；没有观察者时 close 标记未被保存，旧 endpoint 可能继续发起请求 | 改用 send_replace，新增无观察者关闭后的请求拒绝回归，保留已有观察者的关闭验证。 |

## 其他复查结论

Native 维持 Kernel 0.9/v4 的 pointer、冻结分页切点、物理 generation、历史/live 去重与取消结算语义。原生物理存储路径由 Kernel 拥有，客户端不绕过公开协议读 ledger。

DSH 维持 session/follow + page、packed chunks、assistant stream revision/index 与 durable cursor 分离；冷客户端固定切点分页、分支队列清理及 original cwd/current directory 的行为重新通过。两个固定版本仍明确拒绝官方未提供的 ephemeral fork/discard。

Pi 历史继续走 get_entries 活动 ancestry，保留 opaque native entry ID；manifest 是指针/提交意图，不保存 transcript。Goal 激活不会在重连后自动恢复，不确定 mutation 不重发。

## 验证

- 42 项 Rust workspace tests；fmt、严格 Clippy、无 UI/Swift/私有数据库依赖边界检查通过。
- 真实 Kernel 0.9.0、Pi 1.3.0、DSH 0.2.1-rc.2：运行现有全链路合成 provider 验收，新增 Pi 指针保护与 ClientKit 完成 turn 分支验收。
- DSH 0.2.0-rc.2 兼容回归和独立 Kernel 冻结分页/取消结算脚本通过。
- GitHub Actions 的 checks 没有启动任何步骤；平台 annotation 明确为账户付款失败或额度不足。此项是 CI 基础设施阻塞，本地运行了 workflow 中全部对应命令；不宣称 Linux CI 通过。

没有验证付费 provider、生产、远端 TLS、UI、大历史/RSS 或性能。这些边界沿用本轮 headless 验收范围。
