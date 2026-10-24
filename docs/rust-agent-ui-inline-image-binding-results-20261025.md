# #16：原 RunBinding 的无 UI 图片输入

基线 `67b20e6`。设计与所有权见[契约](rust-agent-ui-inline-image-contract-20261025.md)，机器收据见[结果](rust-agent-ui-inline-image-binding-results-20261025.json)。本切片完成公开 API 和本地 SEP fixture 闭环，尚非真实三端图片或图片 UI 验收。

`InlineImageSelection` 在选择前捕获原binding/context/epoch；只读取policy，不要求uploadFile，不伪造上传/回执状态。选中的不可变原bytes在blocking worker中检测PNG/JPEG/GIF/WebP、MIME与尺寸/像素、完整首帧解码；byte/count/mixed-total和允许MIME在大图解码之前检查。整批只做一次原context CAS，文字/settings变化保留，context ABA/重连/迟到拒绝，失败或取消不自动重试。工作内存guard为image crate默认512MiB，部分codec best effort，不宣称严格全codec内存上限。

`PreparedSubmission::attachment_facts()` 与原plan在同锁同draft冻结附件顺序，只新增ID/可缺省metadata，不复制图片base64。`RunBinding::dispatch_prepared_with` 原ScopedPreparation在host模型/auth准备后重读policy，按同一ticket核对mixed file/image：file用冻结facts，image用真实data/实际解码。legacy image无facts可从bytes验证，legacy file无facts且需要大小限制则明确失败。MIME不符的原图不发送。policy检查失败在mark_dispatched前，复用原Completion恢复旧附件并保留后来文字。纯文字/纯file不新增policy查询，原command gate保持；不声称policy和后续open_session/prompt原子绑定，SEP仍最终校验。

**实际结果**：Root380/0、presentation feature289，UI777/0/1既有忽略；feature RunPort19/bindings44，默认/feature严格Clippy、两配置demo构建、standalone公开consumer实际执行、Python262通过。新增core2、图片policy/decoder5、公开RunBinding集成9项；9项包括纯图发送/中文原bytes、无upload能力、坏图批次不部分写入、CAS/epoch、模型准备后混合总量超限零prompt并恢复、未知file facts、旧image追加/真实解码与假MIME拒绝、发送前policy await期间重连。prompt次数由代理endpoint记录真实RPC尝试，未使用不包含prompt的通用fixture mutation账本冒称零发送。

初次验证的测试导入/Arc所有权、fixture单active限制与prompt观测错误均已修正，原失败日志保留；生产校验另经主会话补齐MIME先行与legacy image从真实bytes验证。随后补齐旧image缺facts的追加路径：按真实encoded data而非未知metadata计算混合总量，伪造byte_count也拒绝；定向与完整UI777在该修正后重跑。Root源码未再变化，保留原380/289结果；此前UI776、程序与源码单独保留。直接image依赖使用锁内0.25.10四格式，锁文件只增加既有包依赖边，固定SDK8包/243文件闭包不变。

当前strict reading仍exit1/accepted=false，#53不豁免。真实Native/Pi/DSH provider图片/历史/重连、系统选择/粘贴/拖放、位图预览和窗口/物理IME/OS AX/GPU像素仍not_run；显示基础能力等待TekesUI#117。#16/#17继续开放，PR #10仍draft，不合并/tag/发布。67b20e6两份CI已核对，唯一实际失败#53；本切片新head CI另核对，不能移用旧结论。

证据在SSD `.runtime-fixtures/inline-image-binding-20261025/`：538个source输入、原失败与最终日志、原strict JSON、最终三份程序/SHA及用途、兼容修正前的原程序/收据、独立consumer来源/lock。demo仅构建，9项图片闭环执行的是fixture集成程序；没有把归档程序当真实harness/窗口证据。复用共享target，未新建target，删除缓存0字节。
