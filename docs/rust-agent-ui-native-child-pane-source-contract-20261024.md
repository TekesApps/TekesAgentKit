# #17 / #23：真实 Native 原只读子面板来源闭环

基线 `79727f4`。继续使用默认关闭的 acceptance-source-witness 与已有 child-pane-source CLI，复用原 Sidebar、FamilyCatalog、OpenChild、ChildOpened、ChildRead 和 SidePanel 关闭流程。Native SameSession 是只读来源，不创建独立 child controller、不向子会话发送 Composer 输入。

隔离的已安装 Kernel 0.9.3 连接本机合成 provider。父会话公开 Task 调用产生子 pointer，另建普通辅助会话满足宿主两个主槽位；不能把 child 放进主槽位。先验证种子客户端退出后任务仍在运行，再从公开 catalog 发现 child。只匹配包含 CHILD_NATIVE_PROOF 的子 user 消息，不能误把父工具调用 arguments 中同名字符串当作子请求。失败 gate 最长 30 秒；原面板 running 来源经严格验证后才释放 HTTP 400，超时/abort 不算通过。

原 binding 的 snapshot_with_bounded_source_witness 返回同一次本地读取的 PaneSnapshot 和 typed head 请求/响应。原 snapshot 经宿主 ChildRead admission 挂载，逐项比较 mounted source/scope/publication/epoch/transcript；禁止正常 snapshot 与另一次 raw 查询拼接。running→error 必须同 source Arc、scope、epoch，publication 增长；sourceCall.callId/name/arguments 稳定，phase running→error，原 synthetic Task turn 同步 error。Kernel 不提供 sourceCall.error，不要求或编造该字段。合成 fixture 只允许一个明确的子 turn/start；原 response/error、turn/end(reason:error) 和投影 error item/turn 必须关联该 turn，失败事件必须晚于 running cut，不能用历史无关失败充数。

这是 head-only、只读面板、CPU mount 与公开协议来源一致性证据；不是累计分页来源、网络字节记录、服务端事务或桌面视觉验收。父中文草稿与选中 scope 保留，原控件关闭后旧见证拒绝。复用 SSD 共享 target、固定依赖与宿主身份，不改 TekesUI/Kernel/ClientKit，不触发权限、桌面、物理 IME 或 OS AX。保存失败尝试、程序、固定输入、日志和收据，#53 仍阻止合并/发布。

真实运行首先暴露离屏报告的跨帧竞争：GPU 写 PNG 期间 live binding 可更新，报告不能拿更新后的父快照核对旧树。报告改读原 RunPort::displayed_snapshot，严格 coverage 检查不变；定向回归让原 controller 公开流式输出前进，验证旧树对 displayed 来源通过、对 newer 来源拒绝，更新原控件后再接受 newer。首次失败的双帧、退出码、原程序与日志保留，不能把该次中途 receipt 的 accepted=true 当作整轮成功。
