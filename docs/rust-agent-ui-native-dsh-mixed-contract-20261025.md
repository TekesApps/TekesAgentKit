# #16/#17：Native / DSH mixed 附件的非桌面验收契约

承接 Pi mixed 与三端公开图片 read 的结果。只消费现有 RunBinding、presentation 和固定公开 TekesUI；上游/生产行为不因验收工具而扩展。不读私有 history/DB，不使用桌面或付费模型，不建立新状态机。

两个普通会话分别验证有正文与无正文的 image/file/image。原 AttachmentSelection 实际上传同会话文件，原 InlineImageSelection 捕获两个不同颜色 PNG；原 Composer freeze 必须保留完整顺序及真实 file receipt，再通过原 dispatch 发送。图像和文件都从原 bytes 入手，不能用假 receipt、fixture 路径或只改 metadata 代替上传。

同一轮公开 user 必须是新 source、非 provisional，包含原两张图片和真实文件身份/名称/bytes（按该 SEP 公开历史格式）；最终回复必须属于它自己的 turn。保留 raw content/index/source，不强行套 Pi 的 file metadata。原 runtime 重连后，逐条比较 user/最终 assistant 的 ID/record/source/turn、原附件；每张图片走原 session.attachment 回读。文件仅在 endpoint 实际声明 session.fileAttachment 时公开回读，DSH 无通用文件下载不能伪造；必须保存原上传回执与公开历史引用，不声称模型已读取文件正文。

provider 独立核对两个场景的原 bytes/MIME/顺序、正文与实际文件引用；DSH runtime/time/image-handle/title 的处理只按已核实公开实现，不凭模糊字符串跳过新用户输入。保留所有请求计数与失败，禁止自动重试掩盖。需要转码比对时保留原provider图片与真实ImageIO结果。最终收据只在 owned 服务/进程退出并复核请求计数后写 accepted。

新 example CLI 沿用 `--endpoint-config <单个 Native/DSH 配置 JSON> --receipt <新绝对路径>`，配置提供两个独立 session。固定回文 `ENDPOINT_REPLY_OK`。每次仅一个活动 RunBinding，两个场景共用同一 runtime，再由原 reconnect 路径恢复。

拟定 receipt：`status,endpointKind,roundTrips[2],reconnect`。每个 roundTrip 保留 `scope,rpcId,authoredText,prepared,uploadReceipt,imageInputs,input,assistant,imageReads,fileRead`；prepared 保存原 canonical content/displayText，uploadReceipt 使用原嵌套 `{receiptId,file:{attachmentId,name,bytes}}` DTO；imageReads 保存原历史附件、实际 public method 回读。`fileRead` 明确 `{supported,read}`，无能力时 supported=false/read=null。reconnect 保留 epochBefore/After 及 recovered 两条同结构证据，不编造 provider 观测。外部 runner 另行绑定实际会话与 provider 数据。

主会话统一 Cargo、真实 owned harness 和独立门禁验证，复用 SSD shared-target；实现/验收/CI/发布分开回填。用途不明的数据、源码/WIP、固定 runtime 和证据保留。未实际完成前不记为通过，不关闭 Issue、不合并/tag。

源边界记录：一次已中止的子任务误读了隔离 fixture 私有 history；候选代码已隔离保留，未纳入源码、提交或验收。后续只允许逐项列出的公开源码与公开 receipt，禁止递归搜索 runtime-fixtures。该尝试不计为实现或通过。

已核实公开序列化：Kernel v0.9.3 attachment.rs 按原顺序产生 Block::Image/File；file 为 asset/mime/name/bytes。Responses provider 支持 file 时产生 input_file + filename/file_data，不能当成路径提示；DSH 0.2.1-rc.2 的 session controller 将 receiptId 映射为 `{type:file,attachment}`，dsh-llm replaceFilesWithHandles 在原位置生成精确 fileHandleText。当前阶段只读允许清单在 `.runtime-fixtures/native-dsh-mixed-20261025/public-source/manifest.json`，均为公开发布源码或已核实公开API收据。

真实公开 API 校正：Kernel 的持久 Block::File 经同文件 `project_content_blocks`（1150–1186 行）转换后，公开历史为 `{type:file,attachment:{attachmentId,name,bytes},mediaType}`；图片仍保留 asset/mime/name。探针必须核对公开 DTO，不能把持久存储 Block 格式当作公开历史。前三轮失败证据保留，不计通过。
