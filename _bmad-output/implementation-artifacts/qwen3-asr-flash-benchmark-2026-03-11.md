# Qwen3 ASR Flash Benchmark - 2026-03-11

## Status

Benchmark procedure prepared. Manual utterance collection is still pending.

## Goal

Compare:

- the historical pre-change local Whisper baseline
- the DashScope `qwen3-asr-flash` hosted path

Record the same 9 utterances for both paths using the same latency boundary and trim-only manual accuracy rule.

## Configurations

### Historical Pre-change Local Whisper Baseline

- `provider.kind = "whisper_local"`
- `provider.language = "en"`
- English-only `.en` local model
- `estimated_request_cost_usd = 0.0`

### DashScope Hosted Path

- `provider.kind = "openai_compatible"`
- `base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"`
- `model = "qwen3-asr-flash"`
- `request_mode = "chat_completions"`
- `capability_probe = false`
- `provider.language_hints = ["zh", "en"]` for bilingual runs

## Measurement Rules

- Accuracy rule: compare the raw transcript to the gold transcript after trimming only leading and trailing whitespace.
- Latency rule: measure from the user stop event or VAD flush to successful transcript injection completion.
- Cost rule:
  - local Whisper incremental request cost = `0.00 USD`
  - hosted cost = estimated per-request spend recorded by the operator

## Utterance Matrix

| # | Type | Gold Transcript |
| --- | --- | --- |
| 1 | English | `Please schedule the design review for three p.m. tomorrow.` |
| 2 | English | `The quick brown fox jumps over the lazy dog.` |
| 3 | English | `Open the terminal and run cargo test.` |
| 4 | Mandarin | `请把明天下午三点的会议改到四点。` |
| 5 | Mandarin | `今天天气很好，我们下班后去吃牛肉面。` |
| 6 | Mandarin | `我需要先保存文件，然后再重新启动程序。` |
| 7 | Mixed | `请帮我 open README 然后运行 cargo build。` |
| 8 | Mixed | `这个 bug 在 login flow 里，重现步骤我刚刚发到 Slack 了。` |
| 9 | Mixed | `先切到 whisper_local，再切回 qwen3-asr-flash 做一次 benchmark。` |

## Result Template

### Historical Pre-change Local Whisper Baseline

| # | Raw Transcript | Pass/Fail | Latency (ms) | Cost (USD) | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | TBD | TBD | TBD | 0.00 |  |
| 2 | TBD | TBD | TBD | 0.00 |  |
| 3 | TBD | TBD | TBD | 0.00 |  |
| 4 | TBD | TBD | TBD | 0.00 |  |
| 5 | TBD | TBD | TBD | 0.00 |  |
| 6 | TBD | TBD | TBD | 0.00 |  |
| 7 | TBD | TBD | TBD | 0.00 |  |
| 8 | TBD | TBD | TBD | 0.00 |  |
| 9 | TBD | TBD | TBD | 0.00 |  |

### DashScope qwen3-asr-flash

| # | Raw Transcript | Pass/Fail | Latency (ms) | Cost (USD) | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | TBD | TBD | TBD | TBD |  |
| 2 | TBD | TBD | TBD | TBD |  |
| 3 | TBD | TBD | TBD | TBD |  |
| 4 | TBD | TBD | TBD | TBD |  |
| 5 | TBD | TBD | TBD | TBD |  |
| 6 | TBD | TBD | TBD | TBD |  |
| 7 | TBD | TBD | TBD | TBD |  |
| 8 | TBD | TBD | TBD | TBD |  |
| 9 | TBD | TBD | TBD | TBD |  |

## Summary

Use this section after the manual run to summarize:

- English accuracy
- Mandarin accuracy
- mixed-language handling
- latency delta
- whether the hosted path justifies a later realtime refactor
