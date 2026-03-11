# master - Source Tree Analysis

**Date:** 2026-03-11  
**Scan Level:** focused refresh for hosted-provider refactor

## Relevant Runtime Tree

```text
crates/sttd/src/
├── audio/
├── injection/
├── ipc/
├── provider/
│   ├── mod.rs
│   ├── openai_compatible.rs
│   ├── openrouter.rs
│   ├── whisper_local.rs
│   └── whisper_server.rs
├── debug_wav.rs
├── lib.rs
├── main.rs
├── playback.rs
├── runtime_pipeline.rs
└── state.rs
```

## Integration Points

- `sttctl -> sttd`: Unix socket IPC command/control
- `sttd -> hosted providers`: OpenAI-compatible HTTP, including DashScope `qwen3-asr-flash`
- `sttd -> whisper_server`: HTTP `/inference`
- `sttd -> whisper_local`: process execution
- `sttd -> playerctl/MPRIS`: best-effort global playback pause/resume
- `sttd + sttctl -> common`: shared config and protocol contracts

## File Organization Notes

- hosted provider logic is now centralized in `provider/openai_compatible.rs`
- `provider/openrouter.rs` remains only as a compatibility surface
- final transcription/injection behavior is now isolated in `runtime_pipeline.rs` for direct integration testing
