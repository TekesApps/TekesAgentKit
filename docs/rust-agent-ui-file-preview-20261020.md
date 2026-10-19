# 附件预览接入（实验状态）

基线 AgentKit v0.3.8，TekesUI 继续固定 `431093786de47b48cd566d0d9c0eafa088b10d68`。未修改 TekesUI、ClientKit、Kernel 或 Swift 源码。

上传保存 `AttachmentFacts.attachment_id`，与发送使用的 receiptId 分开。旧草稿兼容反序列化，但缺少读取 ID 时不猜测、不显示预览入口。辅助事实不进入 prompt。

公共 `RunBinding::begin_file_preview` 捕获原 binding、destination、context baseline 和连接 epoch；`read` 调用公开 session.fileAttachment，验证身份、文件名、字节数、base64 上限及媒体类型，返回不可变 bytes。无私有文件/图片接口降级、无自动重试。正文编辑不使读取失效；context ABA、Send、来源退休或连接变化使晚结果失效。

ComposerContext 按文件能力提供“预览附件”，不连带启用 reference/command/skill 来源动作；Composer 的显示与回调共用能力判断。第二公开 consumer 仅消费公开请求和字节结果。

宿主负责导航/IME 屏障和系统预览。ComponentDemo 在 UI 线程最后核对捕获，无异步间隔后启动 `/usr/bin/qlmanage -p`，以独立 argv 传绝对路径；不执行附件或打开默认应用。文件暂存位于唯一目录，Unix 权限目录 0700、文件 0600。宿主保留文件到查看器结束/替换或宿主释放，清理自己的文件及空目录。进程异常退出导致的残留与持久化预览缓存尚未实现。

验证分层：

- 7 项新增 fixture 测试覆盖公开按钮→命令、精确字节/私有暂存清理、跨 binding、context ABA、导航 ABA、合成组字拦截、未知读取 ID 和坏响应。不是物理中文 IME 验收。
- 真实 Native/Pi 经新公开绑定回读字节；DSH 按真实能力不提供通用文件预览。三端上传、正文与纯附件发送、重连后新历史基线和无重复均通过。Pi 历史为文件路径引用，不代表完成 Transcript 中的附件渲染。
- 原生窗口通过真实系统选择器上传 54 B 合成中文附件，点击公开卡片按钮后显示读取成功，并观察到 qlmanage 进程启动。Quick Look 的 AX 读取两次超时，**未验证查看器内的内容视觉效果**。截图和 AX 仅证明原生入口/宿主结果，不能提升为系统预览完整验收。
- 机器收据见 [JSON](rust-agent-ui-file-preview-20261020.json)，原生宿主 [AX](rust-agent-ui-file-preview-native-20261020.txt) 与 [截图](../ui/docs/file-preview-native.png)。

当前仅接草稿通用文件预览。历史附件入口仍需正式 Transcript；图片预览、来源定位/高亮、失败对账与完整物理 IME/三端 pane 原生验收仍未完成。正式 Transcript/RunView 的 TekesUI #22/#23 公共能力阻塞保持不变。

本地最终 gates：UI workspace 149 tests（含第二 consumer）、7 项预览专项、presentation 测试、18 项工具测试、UI all-targets strict Clippy、root/UI fmt、静态依赖边界与 188 个固定输入校验通过。原生内容视觉验收边界见上，不因这些自动化结果提升。
