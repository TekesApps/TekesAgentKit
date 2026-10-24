# #17：纠正 DSH 图片回读验收缺口

承接 `06c0908`。此前图片 probe 硬编码 DSH `publicByteRead=false / unsupported`，没有实际调用公开读取。这是验收程序的错误，不能据此认定 DSH runtime 缺少能力。旧日志、JSON 和程序证据保留，按当轮实际执行解释。

只读核对 ClientKit `c289bc9` 的 `DSHHostEndpoint.attachment` / `DSHRemoteWire.sessionAttachment`，以及 AgentKit 原 `extensions.rs`：两者都已有 `session/attachment` 路由。当前修复只调整 example/验收脚本，**没有增加生产路由或改 SDK/Kernel/ClientKit**。

原 `inline_image_probe` 现从 DSH 原历史描述符取得 attachmentId，通过原 runtime 的 `session.attachment` 读取。三端均核对原 data、ID、MIME、bytes、尺寸；Native 读取 DTO 没有 name，仍用原历史引用证明名称，Pi/DSH 则检查读取 DTO 的 name。独立 Python gate 对发送后与重连后的每份回读都要求真实成功，不接受 unsupported 或省略 read 代替通过。

实际使用同一归档程序连接 Native0.9.3、Pi1.3.0、DSH0.2.1-rc.2：每端两个会话分别图文/纯图，在原 runtime 重连 **2→5** 前后共 **12 次公开图片字节回读**通过。DSH 的 source descriptor 与回读 metadata 一致；原输入/最终回复/source identity 保留。独立 provider 仍为6次图片请求和DSH1次标题请求；原69-byte PNG、实际ImageIO转码与原provider日志留存。

验证：example 严格Clippy/构建通过；Python **289** 项通过（新增4项字节读取门禁回归，包含错误 bytes/MIME/ID/尺寸/name、错误原来源与错误unsupported标记）。生产组件输入未变化，不把 `06c0908` 的 Root388/UI781 等计数伪称本切片重新执行；新 head CI另查。当前严格#53和位图UI#117仍开放，不做桌面/系统IME/OS AX/ProcessContainer或完整UI验收，Native/DSH mixed仍待。

机器记录见[JSON](rust-agent-ui-dsh-image-read-results-20261025.json)。SSD `.runtime-fixtures/dsh-image-read-20261025/` 保存551个代码/配置/测试输入、归档程序及SHA、完整收据/日志。共享target复用，没有新target，删除0 bytes；PR #10仍draft，#16/#17/#28保持开放，不合并/tag。
