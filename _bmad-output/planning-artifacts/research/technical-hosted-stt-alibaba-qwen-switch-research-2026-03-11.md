---
stepsCompleted: [1, 2, 3, 4, 5, 6]
inputDocuments:
  - README.md
  - config/sttd.example.toml
  - config/sttd.env.example
  - crates/common/src/config.rs
  - crates/sttd/src/main.rs
  - crates/sttd/src/provider/mod.rs
  - crates/sttd/src/provider/openrouter.rs
workflowType: 'research'
lastStep: 6
research_type: 'technical'
research_topic: 'Hosted STT provider consolidation plus Alibaba Qwen ASR switch plan for saco-dictation-tool'
research_goals: 'Consolidate prior hosted STT research, deepen Alibaba Qwen ASR integration research, and define how saco-dictation-tool can switch from local Whisper to qwen3-asr-flash and qwen3-asr-flash-realtime.'
user_name: 'Saco'
date: '2026-03-11'
web_research_enabled: true
source_verification: true
supersedes:
  - technical-hosted-multilingual-stt-providers-english-mandarin-2026-03-11.md
  - technical-alibaba-stt-model-comparison-2026-03-11.md
---

# Research Report: technical

**Date:** 2026-03-11  
**Author:** Saco  
**Research Type:** technical

---

## Research Overview

This document replaces the two earlier research artifacts and consolidates:

- the broader hosted multilingual STT provider landscape
- the Alibaba speech model comparison
- a project-specific switch plan for `qwen3-asr-flash`
- a project-specific switch plan for `qwen3-asr-flash-realtime`

The key repo constraint is unchanged:

- today `sttd` providers expose only `transcribe_utterance(...)`
- the runtime captures audio, waits for utterance completion, and then sends one final request
- the current hosted path is optimized for request/response HTTP rather than live WebSocket streaming

## Executive Summary

The high-confidence migration path is now:

1. **near-term switch target:** `qwen3-asr-flash`
2. **next-step realtime target:** `qwen3-asr-flash-realtime`

Why this order:

- `qwen3-asr-flash` aligns with the current repo architecture and can be adopted with limited provider/config refactoring
- `qwen3-asr-flash-realtime` is more likely to beat local Whisper on end-to-end latency, but it requires a new streaming provider/session model

The most important implementation findings are:

1. `qwen3-asr-flash` uses Alibaba’s **OpenAI-compatible Chat Completions** interface, which is already close to the current hosted provider fallback path.
2. `qwen3-asr-flash-realtime` supports both:
   - Alibaba’s native task-style WebSocket protocol
   - an **OpenAI Realtime API-compatible** WebSocket endpoint
3. The current config defaults still force `language = "en"`, which is a bad fit for bilingual English/Mandarin dictation and should be changed before serious benchmarking.
4. The current provider/env naming is OpenRouter-specific even though the hosted abstraction can now stretch beyond OpenRouter.

## Consolidated Provider Position

### Best global hosted candidates

- OpenAI `gpt-4o-mini-transcribe`
- OpenAI `gpt-4o-transcribe`
- Gladia Realtime STT
- Google Chirp 3
- Azure Speech

### Best mainland-China-focused candidates

- Alibaba `qwen3-asr-flash`
- Alibaba `qwen3-asr-flash-realtime`
- Alibaba `paraformer-realtime-v2`
- Alibaba `gummy-chat-v1`
- Alibaba `fun-asr-realtime`
- iFlytek realtime ASR large model

The deeper Alibaba pass changed the priority order. `qwen3-asr-flash` and `qwen3-asr-flash-realtime` are now the first Alibaba models to evaluate.

## Alibaba Speech Model Map

The meaningful Alibaba ASR families are:

- **Qwen ASR**
  - `qwen3-asr-flash`
  - `qwen3-asr-flash-filetrans`
  - `qwen3-asr-flash-realtime`
- **Gummy**
  - `gummy-chat-v1`
  - `gummy-realtime-v1`
- **Fun-ASR**
  - `fun-asr-realtime`
  - `fun-asr-realtime-2026-02-28`
  - `fun-asr`
  - `fun-asr-mtl`
- **Paraformer**
  - `paraformer-realtime-v2`
  - `paraformer-v2`
  - `paraformer-mtl-v1`
- **SenseVoice**
  - `sensevoice-v1`, but it is documented as nearing retirement and should not be selected

## Focus Models

### `qwen3-asr-flash`

Official positioning:

- short-audio ASR
- low-latency
- multilingual, including Mandarin and English
- input via Base64 audio, local file path, or file URL
- OpenAI-compatible Chat Completions API
- optional `stream=true`

This is the most important newly surfaced model because it matches the current `sttd` request/response shape.

### `qwen3-asr-flash-realtime`

Official positioning:

- realtime ASR
- multilingual, including Mandarin and English
- 8 kHz and 16 kHz audio
- OpenAI Realtime API-compatible WebSocket path
- Alibaba native task-style WebSocket path

This is the more aggressive latency path, but it requires runtime and provider changes that the repo does not yet have.

## How `qwen3-asr-flash` Integrates

### Official API shape

Alibaba documents `qwen3-asr-flash` behind the OpenAI-compatible Chat Completions endpoint:

- `POST https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions`
- auth: `Authorization: Bearer <DashScope API key>`
- request carries `input_audio`
- docs show Base64 audio input and optional `stream: true`
- docs also show `extra_body.translation_options.source_language_hints`

That matters because the current provider already builds a very similar payload:

- system instruction
- user text instruction
- `input_audio` with Base64 WAV bytes

### Repo fit

This model is the easiest switch target because:

- `SttProvider` is final-only today
- `process_samples(...)` already collects one utterance and requests one final transcript
- the current hosted adapter already has a `chat/completions` audio-input fallback

### Short-term switch path

The smallest switch path is:

1. keep the current hosted provider implementation temporarily
2. point it at DashScope compatible-mode base URL
3. use model `qwen3-asr-flash`
4. disable capability probing
5. make the provider start directly on `chat/completions` or accept the first-request fallback penalty

Short-term config shape:

```toml
[provider]
kind = "openrouter"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model = "qwen3-asr-flash"
capability_probe = false
```

Temporary env use:

```bash
STTD_OPENROUTER_API_KEY=<dashscope-api-key>
```

This is semantically ugly, but it would work as an interim bridge because the current config/auth fields are OpenRouter-specific.

### Important limitations in the current repo

1. **Language default problem**

The repo defaults `provider.language` to English. That is wrong for bilingual Qwen benchmarking. If left unchanged, it will bias results.

2. **No native `source_language_hints` support**

Alibaba documents `source_language_hints`, but the current request model only exposes a single optional `language` string. That means you cannot express the ideal bilingual hint set such as `["zh", "en"]` without code changes.

3. **Provider naming is misleading**

The current hosted provider is called `openrouter`, but its behavior is now broader than OpenRouter. That will become more confusing once DashScope is a first-class target.

4. **First-request fallback overhead**

The current provider first tries `/audio/transcriptions` and only falls back to `/chat/completions` after an HTTP failure. For DashScope, that probably means the first transcription pays one avoidable failed round trip before the provider caches the chat-completions preference.

## How `qwen3-asr-flash-realtime` Integrates

### Official API shape

Alibaba exposes two integration options:

1. **Native DashScope realtime WebSocket**
   - endpoint documented as `wss://dashscope.aliyuncs.com/api-ws/v1/inference`
   - task-style protocol with `run-task`, `continue-task`, and `finish-task`
2. **OpenAI Realtime API-compatible WebSocket**
   - endpoint documented as `wss://dashscope.aliyuncs.com/compatible-mode/v1/realtime?model=qwen3-asr-flash-realtime`
   - docs show events such as `input_audio_buffer.append`, `input_audio_buffer.commit`, and final transcript delivery via `response.audio_transcript.done`

### Preferred integration path for this repo

Use the **OpenAI Realtime-compatible** path, not the native DashScope task protocol.

Why:

- less Alibaba-specific code
- easier to reason about alongside the existing OpenAI-compatible hosted abstraction
- lower long-term lock-in
- easier future comparison with OpenAI Realtime-style providers

### Repo fit

The current repo cannot directly use `qwen3-asr-flash-realtime` because:

- `SttProvider` has no streaming session lifecycle
- `process_samples(...)` runs only after a full utterance exists
- no IPC or runtime path exists for provider-driven partial transcripts

However, the current runtime already captures audio in chunks:

- push-to-talk stores capture chunks in `ptt_buffer`
- continuous mode captures small chunks before VAD segmentation

That means a staged realtime migration is feasible.

### Recommended staged migration

#### Stage 1: hidden streaming, final-only output

Goal:

- improve latency without changing user-visible partial transcript behavior

Plan:

1. open a realtime provider session when recording starts
2. send audio chunks as they are captured
3. on stop or VAD flush, commit the audio buffer
4. wait for final transcript
5. inject final text exactly as today

This is the lowest-risk realtime migration because it preserves:

- current final-text injection behavior
- current state-machine semantics
- current IPC protocol

#### Stage 2: partial transcript support

Goal:

- show live partials or stable prefixes before final commit

Plan:

1. extend the provider abstraction with streaming events
2. add runtime handling for partial transcript updates
3. add IPC support for partial transcript state
4. add overlay or composition-buffer UX instead of typing mutable partials into the target app

Stage 2 should not come before Stage 1 unless live partial UX is the primary product goal.

## Project Switch Plan

### Phase 0: fix the configuration model first

Recommended changes:

- change default `provider.language` from `Some("en")` to `None`
- add `provider.language_hints: Vec<String>` or equivalent
- add generic hosted provider env names instead of only `STTD_OPENROUTER_*`
- keep backward-compatible aliases if needed

Why this matters:

- bilingual English/Mandarin use needs auto-detect or multiple hints
- the current default fights the intended benchmark

### Phase 1: switch to `qwen3-asr-flash`

Recommended code changes:

1. rename or generalize the `openrouter` provider into a generic OpenAI-compatible hosted provider
2. add a provider option to prefer `chat/completions` from the first request
3. add optional DashScope-specific `source_language_hints`
4. update config examples and env examples for DashScope

Suggested provider behavior:

- for `qwen3-asr-flash`, skip `/audio/transcriptions`
- call `/chat/completions` directly
- reuse the existing WAV-to-Base64 path
- parse transcript from standard chat-completions response

This should deliver the fastest production-usable switch away from local Whisper.

### Phase 2: add `qwen3-asr-flash-realtime`

Recommended code changes:

1. add a streaming provider abstraction beside the current batch `SttProvider`
2. add a dedicated realtime provider module for OpenAI-Realtime-compatible STT
3. start provider sessions from recording start events
4. push audio during capture ticks instead of only after utterance completion
5. first ship final-only commit behavior, then partials later

Suggested abstraction split:

- keep `SttProvider` for utterance/file providers
- add `RealtimeSttProvider` for websocket/session providers

That is cleaner than overloading the current trait with optional behaviors.

## Exact Repo Areas To Change

### For `qwen3-asr-flash`

- `crates/common/src/config.rs`
  - make language truly optional by default
  - add generic hosted API key naming
  - add language hints support
- `config/sttd.example.toml`
  - stop hardcoding English
  - add DashScope example block
- `config/sttd.env.example`
  - add DashScope env example
- `crates/sttd/src/provider/openrouter.rs`
  - generalize provider naming or split out a DashScope/OpenAI-compatible provider
  - support direct chat-completions mode
  - support Qwen `source_language_hints`
- `crates/sttd/src/provider/mod.rs`
  - add a clearer provider kind than `openrouter`

### For `qwen3-asr-flash-realtime`

- `crates/sttd/src/provider/mod.rs`
  - add realtime provider abstraction
- `crates/sttd/src/main.rs`
  - start/stop provider sessions on recording lifecycle
  - push captured audio chunks during runtime ticks
- `crates/sttd/src/state.rs`
  - keep current state model for Stage 1
  - only extend for partial transcript UX in Stage 2
- likely new module
  - `crates/sttd/src/provider/openai_realtime.rs` or similar

## Recommended Order

If the goal is the fastest reliable migration:

1. fix config defaults and language-hint modeling
2. adopt `qwen3-asr-flash`
3. benchmark quality/latency on your actual audio
4. only then decide whether `qwen3-asr-flash-realtime` justifies the larger runtime refactor

If the goal is lowest possible latency and you accept a bigger change:

1. fix config defaults
2. add Stage 1 hidden-streaming support
3. integrate `qwen3-asr-flash-realtime` via the OpenAI-Realtime-compatible WebSocket
4. add partial transcript UX later

## Bottom Line

The correct migration sequence for this project is:

- **first switch:** `qwen3-asr-flash`
- **then evaluate:** `qwen3-asr-flash-realtime`

`qwen3-asr-flash` is the best immediate fit because the current hosted provider already resembles Alibaba’s documented request shape.

`qwen3-asr-flash-realtime` is the better long-term latency play, but it should be implemented as a dedicated streaming path, preferably using Alibaba’s OpenAI-Realtime-compatible endpoint rather than the vendor-specific task protocol.

## Sources

- OpenAI next-generation audio models: https://openai.com/index/introducing-our-next-generation-audio-models/
- OpenAI speech-to-text guide: https://platform.openai.com/docs/guides/speech-to-text
- Google Cloud Chirp 3: https://cloud.google.com/speech-to-text/docs/models/chirp-3
- Google Cloud streaming recognition: https://cloud.google.com/speech-to-text/docs/streaming-recognize
- Google Cloud supported languages: https://cloud.google.com/speech-to-text/docs/speech-to-text-supported-languages
- Azure fast transcription: https://learn.microsoft.com/en-us/azure/ai-services/speech-service/fast-transcription-create
- Azure speech language support: https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-support?tabs=stt
- Gladia realtime quickstart: https://docs.gladia.io/chapters/live-stt/quickstart
- Speechmatics Mandarin-English bilingual announcement: https://www.speechmatics.com/company/articles-and-news/speechmatics-announces-bilingual-speech-recognition-for-mandarin-and-english
- Alibaba speech index: https://help.aliyun.com/zh/model-studio/speech-recognition-api-reference/
- Alibaba realtime speech recognition overview: https://help.aliyun.com/zh/model-studio/real-time-speech-recognition
- Alibaba Qwen file recognition: https://help.aliyun.com/zh/model-studio/qwen-speech-recognition
- Alibaba Qwen realtime speech recognition: https://help.aliyun.com/zh/model-studio/qwen-real-time-speech-recognition
- Alibaba Qwen realtime interaction process: https://help.aliyun.com/zh/model-studio/qwen-asr-realtime-interaction-process
- Alibaba OpenAI-Realtime-compatible realtime speech page: https://help.aliyun.com/zh/model-studio/real-time-speech-recognition-with-openai-realtime-api
- Alibaba recording-file recognition: https://help.aliyun.com/zh/model-studio/recording-file-recognition
- Alibaba model list/specs: https://help.aliyun.com/zh/model-studio/models
- Alibaba pricing: https://help.aliyun.com/zh/model-studio/model-pricing
- Alibaba rate limits: https://help.aliyun.com/zh/model-studio/rate-limit
- Alibaba hotwords: https://help.aliyun.com/zh/model-studio/custom-hot-words/
- iFlytek realtime ASR large model: https://www.xfyun.cn/doc/spark/asr_llm/rtasr_llm.html
- Tencent Cloud realtime speech recognition WebSocket: https://cloud.tencent.com/document/product/1093/48982
