# 限高详情实验与阅读门禁

`OutputMessage::with_bounded_details`、`ToolActivity::with_bounded_details` 是显式实验入口。
标准 RunPort、Transcript 和默认构造器保持原有文档流，等待
[TekesUI #53](https://github.com/TekesApps/TekesUI/issues/53) 修复后再评估启用。

实验沿用原 TextFieldContent、TextSurface、ReadingController 和自动纵向滚动，
按 ClientKit/MetalTextKit 参考采用活动推理最小 72、详情最大 240 个设计点，随
SDK type scale 缩放。正文不受限高影响。原文本所有者、选择和 Composer 组字状态不重建。
工具目前仍是组合后的原始详情字段；这不是完整的 Input/Output 分区或卡片排版验收。

当前精确公开 TekesUI 输入是 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。
普通 UI 工作区检查为 553 passed、1 个已有 #49 ignore，Clippy 通过。
这些检查包括限高、活动推理结束、追加、重排、折叠恢复，以及第二消费者的 Composer 状态隔离。
它们不替代原生系统输入法验收。

独立公开 API 示例 `bounded_detail_reading` 是严格的嵌套阅读检查：子视口向上阅读后，
父容器仍处于 follow latest；追加兄弟消息使父偏移从 0 变为 634，原详情离开视口。
当前结果 `accepted=false`、退出 1。CI 保留该严格检查与 JSON；不得通过忽略、放松断言
或客户端补偿滚动来宣称修复。标准路径的保留也不代表该实验已通过。

证据保存在 `.runtime-fixtures/bounded-detail-20261022/`。
冻结检查程序为 `bounded_detail_reading`，7,836,960 bytes，SHA-256
`80e1ab6c0878b15810d72e2d6ef63cf0c548791014aa43c5193d6667b593b6a2`。
它只依赖系统库，无窗口、GPU 或真实 harness。此阶段复用既有 target；未清理缓存。
