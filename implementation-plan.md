# Implementation Plan

1. 建立 workspace 与三层 crate 结构：`vg-core`、`vg-cli`、`vg-indexer`
2. 实现参数拆分、缓存路径、模型配置持久化
3. 实现文件遍历、变更检测、文本提取、分块、向量入库
4. 实现语义搜索、文本搜索代理、RRF 融合和终端/JSON 输出
5. 补充单元测试、夹具和本地运行说明
