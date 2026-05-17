// SPDX-License-Identifier: GPL-3.0-only
//! Benchmark Qwen GGUF generation through llama.cpp and RLX.
//!
//! Example:
//! ```sh
//! cargo run --release -p skill-llm --features llm-rlx-metal \
//!   --bin bench_qwen_runtimes -- \
//!   --model /path/to/Qwen3-0.6B-Q4_K_M.gguf --runtime all --max-tokens 64
//! ```

#[cfg(feature = "llm")]
fn main() -> anyhow::Result<()> {
    bench::main()
}

#[cfg(not(feature = "llm"))]
fn main() {
    eprintln!("bench_qwen_runtimes requires skill-llm feature: llm");
    std::process::exit(2);
}

#[cfg(feature = "llm")]
mod bench {
    use anyhow::{anyhow, Context, Result};
    use llama_cpp_4::{
        context::params::{LlamaContextParams, LlamaContextType},
        llama_backend::LlamaBackend,
        llama_batch::LlamaBatch,
        model::{params::LlamaModelParams, AddBos, LlamaModel},
        mtp::MtpSession,
        sampling::LlamaSampler,
    };
    use std::{num::NonZeroU32, path::PathBuf, time::Instant};

    #[derive(Debug, Clone)]
    struct Args {
        model: PathBuf,
        prompt: String,
        runtime: Runtime,
        max_tokens: usize,
        ctx_size: u32,
        n_gpu_layers: u32,
        #[cfg_attr(not(feature = "llm-rlx"), allow(dead_code))]
        rlx_device: String,
        #[cfg_attr(not(feature = "llm-rlx"), allow(dead_code))]
        rlx_max_seq: usize,
        mtp_draft_count: u32,
        mtp_n_rs_seq: u32,
        warmup: usize,
        runs: usize,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Runtime {
        All,
        Llama,
        LlamaMtp,
        Rlx,
    }

    #[derive(Debug, Clone)]
    struct RunStats {
        runtime: &'static str,
        load_ms: f64,
        prompt_tokens: usize,
        completion_tokens: usize,
        prefill_ms: Option<f64>,
        decode_ms: f64,
        total_ms: f64,
        mtp_rounds: Option<u64>,
        mtp_drafts: Option<u64>,
        mtp_accepted: Option<u64>,
    }

    pub(super) fn main() -> Result<()> {
        let args = parse_args()?;
        println!("# Qwen runtime benchmark");
        println!("model: {}", args.model.display());
        println!("prompt chars: {}", args.prompt.chars().count());
        println!(
            "max_tokens: {} - ctx_size: {} - runs: {} - warmup: {}",
            args.max_tokens, args.ctx_size, args.runs, args.warmup
        );

        let mut results = Vec::new();

        let llama_needed = matches!(
            args.runtime,
            Runtime::All | Runtime::Llama | Runtime::LlamaMtp | Runtime::Rlx
        );
        let (mut backend, model, prompt_ids) = if llama_needed {
            let mut backend = LlamaBackend::init().context("initialising llama backend")?;
            backend.void_logs();
            let model_params = LlamaModelParams::default().with_n_gpu_layers(args.n_gpu_layers);
            let model = LlamaModel::load_from_file(&backend, &args.model, &model_params)
                .with_context(|| format!("loading llama model {}", args.model.display()))?;
            let prompt_ids = model
                .str_to_token(&args.prompt, AddBos::Always)
                .map_err(|e| anyhow!("tokenizing prompt with llama tokenizer: {e}"))?;
            (Some(backend), Some(model), prompt_ids)
        } else {
            (None, None, Vec::new())
        };

        if matches!(args.runtime, Runtime::All | Runtime::Llama) {
            let backend = backend.as_mut().expect("backend loaded");
            let model = model.as_ref().expect("model loaded");
            let load_start = Instant::now();
            let mut llama = LlamaBench::new(backend, model, &args)?;
            let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
            for _ in 0..args.warmup {
                let _ = llama.run(&prompt_ids, args.max_tokens)?;
            }
            for _ in 0..args.runs {
                let mut s = llama.run(&prompt_ids, args.max_tokens)?;
                s.load_ms = load_ms;
                results.push(s);
            }
        }

        if matches!(args.runtime, Runtime::All | Runtime::LlamaMtp) {
            let backend = backend.as_mut().expect("backend loaded");
            let model = model.as_ref().expect("model loaded");
            let load_start = Instant::now();
            let mut mtp = LlamaMtpBench::new(backend, model, &args)?;
            let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
            for _ in 0..args.warmup {
                let _ = mtp.run(&prompt_ids, args.max_tokens)?;
            }
            for _ in 0..args.runs {
                let mut s = mtp.run(&prompt_ids, args.max_tokens)?;
                s.load_ms = load_ms;
                results.push(s);
            }
        }

        #[cfg(not(feature = "llm-rlx"))]
        if matches!(args.runtime, Runtime::Rlx) {
            return Err(anyhow!("--runtime rlx requires feature llm-rlx"));
        }

        #[cfg(feature = "llm-rlx")]
        if matches!(args.runtime, Runtime::All | Runtime::Rlx) {
            let prompt_u32 = prompt_ids
                .iter()
                .map(|tok| u32::try_from(tok.0).context("negative llama token id cannot be passed to RLX"))
                .collect::<Result<Vec<_>>>()?;
            let load_start = Instant::now();
            let mut rlx = RlxBench::new(&args)?;
            let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
            for _ in 0..args.warmup {
                let _ = rlx.run(&prompt_u32, args.max_tokens)?;
            }
            for _ in 0..args.runs {
                let mut s = rlx.run(&prompt_u32, args.max_tokens)?;
                s.load_ms = load_ms;
                results.push(s);
            }
        }

        print_results(&results);
        Ok(())
    }

    struct LlamaBench<'a> {
        model: &'a LlamaModel,
        ctx: llama_cpp_4::context::LlamaContext<'a>,
    }

    impl<'a> LlamaBench<'a> {
        fn new(backend: &mut LlamaBackend, model: &'a LlamaModel, args: &Args) -> Result<Self> {
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(NonZeroU32::new(args.ctx_size))
                .with_n_batch(args.ctx_size.min(4096))
                .with_n_ubatch(args.ctx_size.min(2048))
                .with_n_threads(-1)
                .with_n_threads_batch(-1)
                .with_flash_attention(true)
                .with_offload_kqv(true);
            let ctx = model
                .new_context(backend, ctx_params)
                .context("creating llama context")?;
            Ok(Self { model, ctx })
        }

        fn run(&mut self, prompt: &[llama_cpp_4::token::LlamaToken], max_tokens: usize) -> Result<RunStats> {
            self.ctx.clear_kv_cache();
            let t_total = Instant::now();
            let t_prefill = Instant::now();
            let n_batch = self.ctx.n_batch() as usize;
            let mut i = 0usize;
            while i < prompt.len() {
                let end = (i + n_batch).min(prompt.len());
                let mut batch = LlamaBatch::new(end - i, 1);
                for (j, &token) in prompt.iter().enumerate().take(end).skip(i) {
                    batch
                        .add(token, j as i32, &[0], j == prompt.len() - 1)
                        .map_err(|_| anyhow!("llama prefill batch overflow"))?;
                }
                self.ctx.decode(&mut batch).context("llama prefill decode")?;
                i = end;
            }
            let prefill_ms = t_prefill.elapsed().as_secs_f64() * 1000.0;

            let t_decode = Instant::now();
            let mut sampler = LlamaSampler::chain_simple([
                LlamaSampler::top_k(40),
                LlamaSampler::top_p(0.9, 1),
                LlamaSampler::temp(0.0),
                LlamaSampler::dist(0xDEAD_BEEF),
            ]);
            let mut n_gen = 0usize;
            let mut pos = prompt.len();
            while n_gen < max_tokens && pos < self.ctx.n_ctx() as usize {
                let token = sampler.sample(&self.ctx, -1);
                sampler.accept(token);
                if self.model.is_eog_token(token) {
                    break;
                }
                let mut batch = LlamaBatch::new(1, 1);
                batch
                    .add(token, pos as i32, &[0], true)
                    .map_err(|_| anyhow!("llama decode batch overflow"))?;
                self.ctx.decode(&mut batch).context("llama decode")?;
                pos += 1;
                n_gen += 1;
            }
            let decode_ms = t_decode.elapsed().as_secs_f64() * 1000.0;
            Ok(RunStats {
                runtime: "llama.cpp",
                load_ms: 0.0,
                prompt_tokens: prompt.len(),
                completion_tokens: n_gen,
                prefill_ms: Some(prefill_ms),
                decode_ms,
                total_ms: t_total.elapsed().as_secs_f64() * 1000.0,
                mtp_rounds: None,
                mtp_drafts: None,
                mtp_accepted: None,
            })
        }
    }

    struct LlamaMtpBench<'a> {
        model: &'a LlamaModel,
        target_ctx: llama_cpp_4::context::LlamaContext<'a>,
        draft_ctx: llama_cpp_4::context::LlamaContext<'a>,
        n_draft_max: i32,
    }

    impl<'a> LlamaMtpBench<'a> {
        fn new(backend: &mut LlamaBackend, model: &'a LlamaModel, args: &Args) -> Result<Self> {
            let n_rs_seq = args.mtp_n_rs_seq.max(args.mtp_draft_count.saturating_add(1)).max(4);
            let target_params = LlamaContextParams::default()
                .with_n_ctx(NonZeroU32::new(args.ctx_size))
                .with_n_batch(args.ctx_size.min(4096))
                .with_n_ubatch(args.ctx_size.min(2048))
                .with_n_threads(-1)
                .with_n_threads_batch(-1)
                .with_flash_attention(true)
                .with_offload_kqv(true)
                .with_ctx_type(LlamaContextType::Default)
                .with_n_rs_seq(n_rs_seq);
            let target_ctx = model
                .new_context(backend, target_params)
                .context("creating llama MTP target context")?;
            let draft_params = LlamaContextParams::default()
                .with_n_ctx(NonZeroU32::new(args.ctx_size))
                .with_n_threads(-1)
                .with_n_threads_batch(-1)
                .with_flash_attention(true)
                .with_offload_kqv(true)
                .with_ctx_type(LlamaContextType::Mtp)
                .with_n_rs_seq(n_rs_seq);
            let draft_ctx = model
                .new_context(backend, draft_params)
                .context("creating llama MTP draft context")?;
            Ok(Self {
                model,
                target_ctx,
                draft_ctx,
                n_draft_max: args.mtp_draft_count as i32,
            })
        }

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        fn run(&mut self, prompt: &[llama_cpp_4::token::LlamaToken], max_tokens: usize) -> Result<RunStats> {
            self.target_ctx.clear_kv_cache();
            self.draft_ctx.clear_kv_cache();
            let t_total = Instant::now();

            let mut session = MtpSession::new(&self.target_ctx, &self.draft_ctx, 1, self.n_draft_max)
                .context("creating MTP session")?;

            let t_prefill = Instant::now();
            let mut prefill = LlamaBatch::new(prompt.len(), 1);
            for (i, &token) in prompt.iter().enumerate() {
                prefill
                    .add(token, i as i32, &[0], i + 1 == prompt.len())
                    .map_err(|_| anyhow!("llama MTP prefill batch overflow"))?;
            }
            self.target_ctx
                .decode(&mut prefill)
                .context("llama MTP prefill decode")?;
            session.process(&prefill).context("MTP process(prefill)")?;
            session.begin(0, prompt).context("MTP begin")?;
            let prefill_ms = t_prefill.elapsed().as_secs_f64() * 1000.0;

            let t_decode = Instant::now();
            let mut sampler = LlamaSampler::chain_simple([
                LlamaSampler::top_k(40),
                LlamaSampler::top_p(0.9, 1),
                LlamaSampler::temp(0.0),
                LlamaSampler::dist(0xDEAD_BEEF),
            ]);

            let first_token = sampler.sample(&self.target_ctx, prefill.n_tokens() - 1);
            sampler.accept(first_token);
            if self.model.is_eog_token(first_token) {
                let decode_ms = t_decode.elapsed().as_secs_f64() * 1000.0;
                return Ok(RunStats {
                    runtime: "llama.cpp-mtp",
                    load_ms: 0.0,
                    prompt_tokens: prompt.len(),
                    completion_tokens: 0,
                    prefill_ms: Some(prefill_ms),
                    decode_ms,
                    total_ms: t_total.elapsed().as_secs_f64() * 1000.0,
                    mtp_rounds: Some(0),
                    mtp_drafts: Some(0),
                    mtp_accepted: Some(0),
                });
            }

            let mut last_token = first_token;
            let mut n_past = prompt.len() as i32;
            let mut n_gen = 1usize;
            let mut rounds = 0u64;
            let mut drafts_total = 0u64;
            let mut accepted_total = 0u64;
            let verify_cap = (self.n_draft_max as usize + 1).max(self.target_ctx.n_batch() as usize);
            let mut verify = LlamaBatch::new(verify_cap, 1);

            'gen: while n_gen < max_tokens && (n_past as usize) < self.target_ctx.n_ctx() as usize {
                let drafts = session.draft(0, n_past, last_token).context("MTP draft")?;
                rounds += 1;
                drafts_total += drafts.len() as u64;

                verify.clear();
                verify
                    .add(last_token, n_past, &[0], true)
                    .map_err(|_| anyhow!("llama MTP verify batch overflow"))?;
                for (i, d) in drafts.iter().enumerate() {
                    verify
                        .add(*d, n_past + 1 + i as i32, &[0], true)
                        .map_err(|_| anyhow!("llama MTP verify batch overflow"))?;
                }
                let n_verify = verify.n_tokens();

                self.draft_ctx
                    .clear_kv_cache_seq(Some(0), Some(n_past as u32), None)
                    .context("MTP draft KV rollback")?;
                self.target_ctx.decode(&mut verify).context("llama MTP verify decode")?;
                session.process(&verify).context("MTP process(verify)")?;

                let mut n_accepted = 0usize;
                let mut next_token = sampler.sample(&self.target_ctx, 0);
                sampler.accept(next_token);
                for (i, draft) in drafts.iter().enumerate() {
                    if next_token == *draft {
                        n_accepted = i + 1;
                        if i + 1 < n_verify as usize {
                            next_token = sampler.sample(&self.target_ctx, (i + 1) as i32);
                            sampler.accept(next_token);
                        }
                    } else {
                        break;
                    }
                }
                accepted_total += n_accepted as u64;

                let new_n_past = n_past + 1 + n_accepted as i32;
                if n_accepted < drafts.len() {
                    if !self
                        .target_ctx
                        .clear_kv_cache_seq(Some(0), Some(new_n_past as u32), None)
                        .context("MTP target KV rollback")?
                    {
                        return Err(anyhow!("MTP target KV rollback rejected"));
                    }
                    if !self
                        .draft_ctx
                        .clear_kv_cache_seq(Some(0), Some(new_n_past as u32), None)
                        .context("MTP draft KV rollback")?
                    {
                        return Err(anyhow!("MTP draft KV rollback rejected"));
                    }
                }
                session.accept(0, n_accepted as u16).context("MTP accept")?;

                for d in drafts.iter().take(n_accepted) {
                    if self.model.is_eog_token(*d) {
                        break 'gen;
                    }
                    n_gen += 1;
                    if n_gen >= max_tokens {
                        break 'gen;
                    }
                }
                if self.model.is_eog_token(next_token) {
                    break;
                }
                n_gen += 1;
                last_token = next_token;
                n_past = new_n_past;
            }

            let decode_ms = t_decode.elapsed().as_secs_f64() * 1000.0;
            Ok(RunStats {
                runtime: "llama.cpp-mtp",
                load_ms: 0.0,
                prompt_tokens: prompt.len(),
                completion_tokens: n_gen,
                prefill_ms: Some(prefill_ms),
                decode_ms,
                total_ms: t_total.elapsed().as_secs_f64() * 1000.0,
                mtp_rounds: Some(rounds),
                mtp_drafts: Some(drafts_total),
                mtp_accepted: Some(accepted_total),
            })
        }
    }

    #[cfg(feature = "llm-rlx")]
    struct RlxBench {
        runner: rlx::run::Qwen3Runner,
    }

    #[cfg(feature = "llm-rlx")]
    impl RlxBench {
        fn new(args: &Args) -> Result<Self> {
            let device = parse_rlx_device(&args.rlx_device)?;
            let runner = rlx::run::Qwen3Runner::builder()
                .weights(&args.model)
                .device(device)
                .max_seq(args.rlx_max_seq)
                .stream(false)
                .build()
                .context("building RLX Qwen runner")?;
            Ok(Self { runner })
        }

        fn run(&mut self, prompt: &[u32], max_tokens: usize) -> Result<RunStats> {
            let t = Instant::now();
            let out = self
                .runner
                .generate(prompt, max_tokens, |_| {})
                .context("RLX generate")?;
            let total_ms = t.elapsed().as_secs_f64() * 1000.0;
            Ok(RunStats {
                runtime: "rlx",
                load_ms: 0.0,
                prompt_tokens: prompt.len(),
                completion_tokens: out.len(),
                prefill_ms: None,
                decode_ms: total_ms,
                total_ms,
                mtp_rounds: None,
                mtp_drafts: None,
                mtp_accepted: None,
            })
        }
    }

    fn parse_args() -> Result<Args> {
        let mut model = None;
        let mut prompt = "Write one concise paragraph about brain-computer interfaces.".to_string();
        let mut runtime = Runtime::All;
        let mut max_tokens = 64usize;
        let mut ctx_size = 2048u32;
        let mut n_gpu_layers = u32::MAX;
        let mut rlx_device = if cfg!(target_os = "macos") { "metal" } else { "cpu" }.to_string();
        let mut rlx_max_seq = 128usize;
        let mut mtp_draft_count = 1u32;
        let mut mtp_n_rs_seq = 0u32;
        let mut warmup = 1usize;
        let mut runs = 3usize;

        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0usize;
        while i < args.len() {
            let key = &args[i];
            let mut value = || -> Result<String> {
                i += 1;
                args.get(i).cloned().ok_or_else(|| anyhow!("missing value for {key}"))
            };
            match key.as_str() {
                "--model" => model = Some(value()?.into()),
                "--prompt" => prompt = value()?,
                "--runtime" => {
                    runtime = match value()?.as_str() {
                        "all" => Runtime::All,
                        "llama" | "llama.cpp" => Runtime::Llama,
                        "mtp" | "llama-mtp" | "llama.cpp-mtp" => Runtime::LlamaMtp,
                        "rlx" => Runtime::Rlx,
                        other => return Err(anyhow!("--runtime must be all|llama|mtp|rlx, got {other}")),
                    };
                }
                "--max-tokens" => max_tokens = value()?.parse()?,
                "--ctx-size" => ctx_size = value()?.parse()?,
                "--n-gpu-layers" => {
                    let raw: i64 = value()?.parse()?;
                    n_gpu_layers = if raw < 0 { u32::MAX } else { raw as u32 };
                }
                "--rlx-device" => rlx_device = value()?,
                "--rlx-max-seq" => rlx_max_seq = value()?.parse()?,
                "--mtp-draft-count" => mtp_draft_count = value()?.parse()?,
                "--mtp-n-rs-seq" => mtp_n_rs_seq = value()?.parse()?,
                "--warmup" => warmup = value()?.parse()?,
                "--runs" => runs = value()?.parse()?,
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                other => return Err(anyhow!("unknown flag {other}")),
            }
            i += 1;
        }

        Ok(Args {
            model: model.ok_or_else(|| anyhow!("--model /path/to/model.gguf is required"))?,
            prompt,
            runtime,
            max_tokens,
            ctx_size,
            n_gpu_layers,
            rlx_device,
            rlx_max_seq,
            mtp_draft_count,
            mtp_n_rs_seq,
            warmup,
            runs,
        })
    }

    #[cfg(feature = "llm-rlx")]
    fn parse_rlx_device(tag: &str) -> Result<rlx::Device> {
        match tag.to_ascii_lowercase().as_str() {
            "cpu" => Ok(rlx::Device::Cpu),
            "metal" => Ok(rlx::Device::Metal),
            "mlx" => Ok(rlx::Device::Mlx),
            "gpu" | "wgpu" => Ok(rlx::Device::Gpu),
            "cuda" => Ok(rlx::Device::Cuda),
            "rocm" => Ok(rlx::Device::Rocm),
            "tpu" => Ok(rlx::Device::Tpu),
            other => Err(anyhow!("unsupported RLX device '{other}'")),
        }
    }

    fn print_results(results: &[RunStats]) {
        println!();
        println!("| runtime | load ms | prompt tok | gen tok | prefill ms | decode/total ms | tok/s | mtp accept |");
        println!("|---|---:|---:|---:|---:|---:|---:|---:|");
        for r in results {
            let tok_s = if r.decode_ms > 0.0 {
                r.completion_tokens as f64 / (r.decode_ms / 1000.0)
            } else {
                0.0
            };
            let prefill = r.prefill_ms.map(|v| format!("{v:.1}")).unwrap_or_else(|| "n/a".into());
            let mtp_accept = match (r.mtp_rounds, r.mtp_drafts, r.mtp_accepted) {
                (Some(rounds), Some(drafts), Some(accepted)) if drafts > 0 => {
                    format!(
                        "{accepted}/{drafts} ({:.1}%, {rounds} rounds)",
                        100.0 * accepted as f64 / drafts as f64
                    )
                }
                (Some(rounds), Some(_), Some(_)) => format!("0/0 (0.0%, {rounds} rounds)"),
                _ => "n/a".into(),
            };
            println!(
                "| {} | {:.1} | {} | {} | {} | {:.1} | {:.2} | {} |",
                r.runtime, r.load_ms, r.prompt_tokens, r.completion_tokens, prefill, r.total_ms, tok_s, mtp_accept
            );
        }
    }

    fn print_usage() {
        eprintln!(
            "Usage: bench_qwen_runtimes --model model.gguf [--runtime all|llama|mtp|rlx] \
             [--prompt TEXT] [--max-tokens N] [--ctx-size N] [--n-gpu-layers -1] \
             [--mtp-draft-count 1] [--mtp-n-rs-seq 0] [--rlx-device metal] \
             [--rlx-max-seq 128] [--warmup 1] [--runs 3]"
        );
    }
}
