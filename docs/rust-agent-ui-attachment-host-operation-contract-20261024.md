# #16：附件宿主操作共用契约

关联 [#16](https://github.com/TekesApps/TekesAgentKit/issues/16)、[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。仅改AgentKit demo自有宿主，不改Kit业务状态机、ClientKit/Tekes/Kernel/TekesUI。复用公开精确TekesUI50ec49f与SSD共享target，桌面仍不跑。

ClientKit c289bc9 的 ComposerCardView.presentAttachmentImporter/appendAttachments 负责选择结果与附件准备，原controlState/目的地拥有草稿；Tekes8858ccd ClientAgentHostAdapters仅提供导航、产品能力等宿主输入。本轮不复制Swift实现，也不声称系统选择器已验收。旧三端附件收据证明原binding链，不能证明当时离屏手工read/upload/complete执行了真实宿主的同一个materialize操作。

本轮提取 demo-local select_and_upload：原Arc<AttachmentSelection> + Option<Vec<PathBuf>> → spawn_blocking原有受限read_selected_files → 原request.upload → AttachmentHostCompletion。None原位cancel_selection；读文件/Join错误原位fail_selection，均不上传。completion私有保存原request/files/result，apply消费self并只调用原ComponentDemo.complete_file_selection，继续执行原身份/迟到门禁。文件字节不进入UI组件；宿主保留文件与报告供原重试/恢复状态使用。

真实RFD仍在主UI线程构造，选择后仅把可选paths交给此操作；HostResult只携带typed completion，在原消息处理入口apply。离屏upload_proof也调用相同操作与apply，保留真实policy/upload/receipt、one-shot拒绝、A/B草稿、正文加附件/纯附件发送、公开历史及重连去重断言。没有测试特供状态机、自动重试或新公共Kit API。

验收分层：

- 定向fixture验证取消/读取失败不上传；上传被受控gate保持pending时切B，完成后只更新原A且保留B草稿。既有Composer/pane/attachment测试继续运行。
- 新runner只接显式已有headless/demo、已安装Kernel0.9.3/Pi1.3.0/DSH0.2.1-rc.2与固定providers快照；闭合env、owned HOME/data和合成loopback provider。DSH query_sqlite=False，不读harness私有session文件；公开seed两会话后逐端执行当前demo的原upload-proof离屏路径。
- 原结构化收据必须声明共享操作/typed completion实际完成、系统选择器未运行；旧近似路径收据不能通过新校验器。Native/Pi通用字节回读及跨session拒绝按公开capability核对；DSH不能把无公开字节接口记成拒绝成功。
- 验证当前demo、适用UI回归与独立公开consumer；冻结源/程序/hash和原始日志。ownedKernel/DSH退出后才写整体accepted；不伪造Pi子进程退出收据。

OS选择器交互、系统权限、桌面/物理IME/OS AX、完整布局/焦点/ProcessContainer仍not_run，不能据本切片关闭#16或解除#53严格阅读门。实际结果另写，不以本契约作为通过证明。
