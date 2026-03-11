#![allow(unused_crate_dependencies)]

use anyhow as _;
use clap as _;
use tracing_subscriber as _;

pub mod audio;
pub mod debug_wav;
pub mod injection;
pub mod ipc;
pub mod playback;
pub mod provider;
pub mod runtime_pipeline;
pub mod state;
