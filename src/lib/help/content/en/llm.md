# Overview
NeuroSkill ships an optional local LLM server that gives you a private, OpenAI-compatible AI assistant without sending any data to the cloud.

## What is the LLM feature?
The LLM feature embeds a llama.cpp-backed inference server directly inside the app. When enabled, it serves OpenAI-compatible endpoints (/v1/chat/completions, /v1/completions, /v1/embeddings, /v1/models, /health) on the same local port as the WebSocket API. You can point any OpenAI-compatible client — Chatbot UI, Continue, Open Interpreter, or your own scripts — at it.

## Privacy & Offline Use
All inference runs on your machine. No tokens, prompts, or completions ever leave localhost. The only network activity is the initial model download from HuggingFace Hub. Once a model is cached locally you can disconnect from the internet entirely.

## OpenAI-Compatible API
The server speaks the same protocol as the OpenAI API. Any library that accepts a base_url parameter (openai-python, openai-node, LangChain, LlamaIndex, etc.) works out of the box. Set base_url to http://localhost:<port>/v1 and leave the API key empty unless you configured one in Inference Settings.

# Model Management
Browse, download, and activate GGUF-quantised language models from the built-in catalog.

## Model Catalog
The catalog lists curated model families (e.g. Qwen, Llama, Gemma, Phi) with multiple quantisation variants per family. Use the family dropdown to browse, then pick a specific quant to download. Models marked with ★ are the recommended default for that family.

## Quantisation Levels
Each model is available in several GGUF quantisation levels (Q4_K_M, Q5_K_M, Q6_K, Q8_0, etc.). Lower quants are smaller and faster but sacrifice some quality. Q4_K_M is usually the best trade-off. Q8_0 is near-lossless but requires roughly twice the memory. BF16/F16/F32 are unquantised reference weights.

## Hardware Fit Badges
Each quant row shows a colour-coded badge estimating how well it fits your hardware: 🟢 Runs great — fits fully in GPU VRAM with headroom. 🟡 Runs well — fits in VRAM with a tight margin. 🟠 Tight fit — may need partial CPU offload or reduced context size. 🔴 Won't fit — too large for available memory. The estimate considers GPU VRAM, system RAM, model size, and context overhead.

## Vision / Multimodal Models
Families tagged Vision or Multimodal include an optional multimodal projector (mmproj) file. Download both the text model and its projector to enable image input in the chat window. The projector extends the text model — it is not a standalone model.

## Downloading & Deleting
Click 'Download' to fetch a model from HuggingFace Hub. A progress bar shows real-time download status. You can cancel at any time. Downloaded models are stored locally and can be deleted to free disk space. Use the 'Refresh cache' button to re-scan the catalog if you manually modify the model directory.

# Inference Settings
Fine-tune how the server loads and runs models.

## GPU Layers
Controls how many transformer layers are offloaded to GPU. Set to 'All' for maximum speed if the model fits in VRAM. Set to 0 for CPU-only inference. Intermediate values split the model across GPU and CPU — useful when the model barely exceeds VRAM capacity.

## Context Size
The KV-cache size in tokens. 'Auto' picks the largest context that fits your GPU/RAM based on the model's size and quantization. Larger contexts let the model remember more conversation history but consume more memory. Options are limited to the model's trained maximum. If you run into out-of-memory errors, try reducing context size.

## Parallel Requests
Maximum number of concurrent decode loops. Higher values let multiple clients share the server but increase peak memory usage. For most single-user setups, 1 is fine.

## API Key
An optional Bearer token required on every /v1/* request. Leave empty for open access on localhost. Set a key if you expose the port on a local network and want to restrict access.

# Built-in Tools
The LLM chat can call local tools to gather information or take actions on your behalf.

## How Tools Work
When tool use is enabled, the model can request to call one or more tools during a conversation. The app executes the tool locally and feeds the result back to the model so it can incorporate real-world information into its response. Tools are only invoked when the model explicitly requests them — they never run in the background.

## Safe Tools
Date, Location, Web Search, Web Fetch, and Read File are read-only tools that cannot modify your system. Date returns the current local date and time. Location provides approximate IP-based geolocation. Web Search runs a DuckDuckGo instant-answer query. Web Fetch retrieves the text body of a public URL. Read File reads local files with optional pagination.

## Privileged Tools (⚠️)
Bash, Write File, and Edit File can modify your system. Bash executes shell commands with the same permissions as the app. Write File creates or overwrites files on disk. Edit File performs find-and-replace edits. These are disabled by default and show a warning badge. Enable them only if you understand the risks.

## Execution Mode & Limits
Parallel mode lets the model call multiple tools at once (faster). Sequential mode runs them one at a time (safer for tools with side-effects). 'Max rounds' limits how many tool-call / tool-result round trips are allowed per message. 'Max calls per round' caps the number of simultaneous tool invocations.

# Chat & Logs
Interact with the model and monitor server activity.

## Chat Window
Open the chat window from the LLM server card or the tray menu. It provides a familiar chat interface with markdown rendering, code highlighting, and tool-call visualisation. Conversations are ephemeral — they are not saved to disk. Vision-capable models accept image attachments via drag-and-drop or the attachment button.

## Using External Clients
Because the server is OpenAI-compatible, you can use any external chat frontend. Point it at http://localhost:<port>/v1, set an API key if you configured one, and select any model name from /v1/models. Popular options include Open WebUI, Chatbot UI, Continue (VS Code), and curl / httpie for scripting.

## Server Logs
The log viewer at the bottom of the LLM settings panel streams server output in real time. It shows model loading progress, token generation speed, and any errors. Enable 'Verbose' mode in the advanced section for detailed llama.cpp diagnostic output. Logs auto-scroll but you can pause by scrolling up manually.
