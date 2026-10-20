# TekesUI 阅读事务候选：输入接收

收到 #22 的独立本地交付包，导出身份 `bd3ed1b0af81b044306af90d3594bb31110147b7194bda6053b1d2e185871c9a`。已独立验证压缩包 SHA-256 `f3ed66b85e31375582074eec2c8903904aa14649dae1cdef8f6bef2057dada61`，并在不解包执行的情况下逐文件检查449个输入均与 manifest 一致，无重复路径或链接。输入副本保存于忽略目录 `.runtime-fixtures/tekesui-reading-bd3ed1b0af81/inputs/`；记录见[JSON](tekesui-reading-candidate-intake-20261020.json)。

本次只确认输入完整性，没有构建、消费者接入、原生验收或候选提升。默认依赖仍为公开固定 revision `8cf0840790334ba224afc0595897c74231c712e6`，没有替换既有冻结输入，也不以发送方验证替代接收方结论。该包不是公开 Git pin，registry 依赖仍需 cache/network。

下一步按独立候选工作区执行 REPLAY.md：先验证公开阅读契约、第二消费者、Clippy 与 GPU，再把 AgentKit 的稳定消息/内容身份和已有分页 intent 适配到 ContentHost/ScrollContent 的公开阅读事务。库负责 item-local 测量、恢复与 fallback；AgentKit 不计算像素补偿，不把 item-local 点当作任意控件的字符/UTF-16 锚点。回到最新使用公开 preserving_focus 能力，不能清除 Composer 焦点或组字。

新版新增 CompositionError 枚举项，适配时须检查穷举匹配。原生动量/IME/AX、ClientKit 同条件布局，以及公开精确 pin 和独立提升结论仍需分别完成；#22 不能据此关闭。#25 的富内容能力另行处理。本轮的三端取消/错误/恢复证据继续只对应现有默认 pin。

后续状态：[独立回放与 AgentKit Transcript 联调](rust-agent-ui-reading-candidate-20261020.md)已完成专项公开 API/几何验证；本文件及原 JSON 保留接收时的完整性检查范围，后续证据另记。默认 pin 与原生验收状态不变。
