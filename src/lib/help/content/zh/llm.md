# 概述
NeuroSkill 附带一个可选的本地 LLM 服务器，让您拥有一个私人的、OpenAI 兼容的 AI 助手，无需向云端发送任何数据。

## 什么是 LLM 功能？
LLM 功能在应用内嵌入了一个由 llama.cpp 驱动的推理服务器。启用后，它在与 WebSocket API 相同的本地端口上提供 OpenAI 兼容端点（/v1/chat/completions、/v1/completions、/v1/embeddings、/v1/models、/health）。您可以将任何 OpenAI 兼容客户端——Chatbot UI、Continue、Open Interpreter 或您自己的脚本——指向它。

## 隐私与离线使用
所有推理在您的设备上运行。没有令牌、提示或补全离开 localhost。唯一的网络活动是从 HuggingFace Hub 的初始模型下载。模型本地缓存后，您可以完全断开互联网。

## OpenAI 兼容 API
服务器使用与 OpenAI API 相同的协议。任何接受 base_url 参数的库（openai-python、openai-node、LangChain、LlamaIndex 等）都可以直接使用。将 base_url 设为 http://localhost:<port>/v1，API 密钥留空，除非您在推理设置中配置了密钥。

# 模型管理
浏览、下载和激活内置目录中 GGUF 量化的语言模型。

## 模型目录
目录列出精选的模型系列（如 Qwen、Llama、Gemma、Phi），每个系列有多种量化变体。使用系列下拉菜单浏览，然后选择特定量化版本下载。标有 ★ 的模型是该系列的推荐默认选项。

## 量化级别
每个模型有多种 GGUF 量化级别（Q4_K_M、Q5_K_M、Q6_K、Q8_0 等）。较低的量化更小更快，但会牺牲一些质量。Q4_K_M 通常是最佳权衡。Q8_0 接近无损但需要大约两倍内存。BF16/F16/F32 是未量化的参考权重。

## 硬件适配徽章
每个量化行显示一个颜色编码的徽章，估计它与您硬件的适配程度：🟢 运行极佳——完全适配 GPU VRAM 且有余量。🟡 运行良好——适配 VRAM 但余量较紧。🟠 紧凑适配——可能需要部分 CPU 卸载或减小上下文大小。🔴 无法适配——对于可用内存来说太大。估算考虑了 GPU VRAM、系统 RAM、模型大小和上下文开销。

## 视觉/多模态模型
标记为视觉或多模态的系列包含一个可选的多模态投影器（mmproj）文件。下载文本模型及其投影器以在聊天窗口中启用图像输入。投影器扩展文本模型——它不是独立模型。

## 下载与删除
Click 'Download' to fetch a model from HuggingFace Hub. A progress bar shows real-time download status. You can cancel at any time. Downloaded models are stored locally and can be deleted to free disk space. Use the 'Refresh cache' button to re-scan the catalog if you manually modify the model directory.

# 推理设置
微调服务器加载和运行模型的方式。

## GPU 层数
Controls how many transformer layers are offloaded to GPU. Set to 'All' for maximum speed if the model fits in VRAM. Set to 0 for CPU-only inference. Intermediate values split the model across GPU and CPU — useful when the model barely exceeds VRAM capacity.

## 上下文大小
The KV-cache size in tokens. 'Auto' picks the largest context that fits your GPU/RAM based on the model's size and quantization. Larger contexts let the model remember more conversation history but consume more memory. Options are limited to the model's trained maximum. If you run into out-of-memory errors, try reducing context size.

## 并行请求
最大并发解码循环数。较高的值允许多个客户端共享服务器，但增加峰值内存使用。对于大多数单用户设置，1 就足够了。

## API 密钥
每个 /v1/* 请求所需的可选 Bearer 令牌。留空表示在 localhost 上开放访问。如果您在局域网上暴露端口并希望限制访问，请设置密钥。

# 内置工具
LLM 聊天可以调用本地工具来收集信息或代您执行操作。

## 工具如何工作
启用工具使用后，模型可以在对话中请求调用一个或多个工具。应用在本地执行工具并将结果反馈给模型，以便它将真实信息纳入响应中。工具仅在模型明确请求时调用——它们从不在后台运行。

## 安全工具
日期、位置、网页搜索、网页获取和读取文件是只读工具，不会修改您的系统。日期返回当前本地日期和时间。位置提供基于 IP 的近似地理位置。网页搜索运行 DuckDuckGo 即时回答查询。网页获取检索公共 URL 的文本正文。读取文件读取本地文件，支持可选分页。

## 特权工具（⚠️）
Bash、写入文件和编辑文件可以修改您的系统。Bash 以与应用相同的权限执行 shell 命令。写入文件在磁盘上创建或覆盖文件。编辑文件执行查找和替换编辑。这些默认禁用并显示警告徽章。仅在您了解风险时才启用它们。

## 执行模式与限制
Parallel mode lets the model call multiple tools at once (faster). Sequential mode runs them one at a time (safer for tools with side-effects). 'Max rounds' limits how many tool-call / tool-result round trips are allowed per message. 'Max calls per round' caps the number of simultaneous tool invocations.

# 聊天与日志
与模型交互并监控服务器活动。

## 聊天窗口
从 LLM 服务器卡片或托盘菜单打开聊天窗口。它提供熟悉的聊天界面，支持 Markdown 渲染、代码高亮和工具调用可视化。对话是临时的——不保存到磁盘。支持视觉功能的模型通过拖放或附件按钮接受图像附件。

## 使用外部客户端
由于服务器兼容 OpenAI，您可以使用任何外部聊天前端。将其指向 http://localhost:<port>/v1，如果配置了 API 密钥则设置密钥，并从 /v1/models 选择任何模型名称。热门选项包括 Open WebUI、Chatbot UI、Continue（VS Code）以及用于脚本的 curl / httpie。

## 服务器日志
The log viewer at the bottom of the LLM settings panel streams server output in real time. It shows model loading progress, token generation speed, and any errors. Enable 'Verbose' mode in the advanced section for detailed llama.cpp diagnostic output. Logs auto-scroll but you can pause by scrolling up manually.
