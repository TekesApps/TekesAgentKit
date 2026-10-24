# #16：共享附件宿主操作的真实三端验证

关联 [#16](https://github.com/TekesApps/TekesAgentKit/issues/16)、[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。[短契约](rust-agent-ui-attachment-host-operation-contract-20261024.md) · [机器收据](rust-agent-ui-attachment-host-operation-results-20261024.json)。只改AgentKit demo宿主，UI仍试验；公开TekesUI精确pin50ec49f不变。

## 本轮完成

真实RFD路径与离屏upload_proof现共用同一demo-local操作：可选paths→阻塞池受限文件读取→原Arc请求上传→typed completion→原complete_file_selection身份与迟到门禁。取消/读取失败在原request上结束，不上传；原文件/报告保留，原Kit业务状态机和authority不改。主会话审查实施子任务改动，并把导航测试收紧为上传pending期间切换会话后才放行结果。

新增runner只消费现有headless与当前demo、已安装Kernel0.9.3、Pi1.3.0、DSH0.2.1-rc.2及固定providers快照。闭合env/owned HOME/data，loopback合成provider，DSH明确关闭query_sqlite插件；公开seed两个会话后逐端走原组件入口。没有执行无关旧生命周期或读私有session文件。

两次真实三端运行均通过；加强收据拒绝条件后的final为14次provider请求。每端都完成原上传policy/receipt、正文加附件和纯附件Send、one-shot重复上传拒绝、切B后原A回传与B草稿保留、公开历史及新epoch重连去重。Native/Pi公开通用字节回读与跨session拒绝通过；DSH无此公开能力，保留readUnavailable=true及foreignSessionReadRejected=null，没有冒称拒绝成功。ownedKernel/DSH均退出0，之后写整体accepted；Pi由公开消费者正常退出，没有单独伪造Node退出状态。

## 实际验证

- 新3项fixture验证取消、缺失文件读取失败均零上传，以及受控pending上传期间导航后的原请求身份/目的地草稿隔离。5个附件相关套件共26/0/0通过，包括原pane关闭、同scope换owner、恢复状态等回归。
- UI全套750/0/1既有忽略，Python207/0（3项新收据拒绝测试），严格UI Clippy、格式、边界与验收矩阵通过；普通demo构建通过。只改demo，没有重跑未修改的root346/feature254，不把上一切片的运行扩报为本轮新执行。
- 复用同一SSD独立公开consumer fixture并在本切片重新构建/执行exit0，6挂载原断言通过；137文件公开来源闭包仍为721c1d36，与DSH冻结来源完全一致，消费SDK243文件闭包仍为3ad8aa98。没有另建target。

SSD `.runtime-fixtures/attachment-host-operation-20261024/` 保留verified/final两轮的原stdout/stderr、commands/公开见证与uploadProof、PNG离屏渲染、owned退出、provider路径（不保存请求body）、固定程序hash和源码。当前demo程序已归档；headless引用DSH切片已有同hash归档，避免重复复制。历史证据与固定依赖安装保留。

本轮证明真实宿主与验收共用的materialize/上传/完成逻辑；**没有运行系统文件选择器或原生窗口HostResult消息循环**，也未完成桌面/物理IME/OS AX、完整布局/焦点或ProcessContainer。此项只补#16的非桌面部分，Issue仍开放，#53严格门保留，不合并/tag/发布。新提交CI需另核对。

全部构建复用SSD共享target并关闭增量编译，本切片删除缓存0字节；没有打断其他任务、创建新的应用身份或触发Screen Recording。

CI后续核对：4495f35的PR40092397118/push40092394154均终态。root346/0、opt-in254/0、UI750/0/1、Python207与独立consumer实际步骤通过；唯一失败仍为严格#53（accepted=false，父追加offset634、detail不可见），CI普通demo步骤因此跳过，本轮本地构建证据独立保留。两份原artifact/log、1055提交输入以及137/243文件闭包已核对，PR merge02c02b42父包含4495。此证据只属于4495，不替代后续提交CI。
