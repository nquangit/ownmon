//! Core monitoring logic.
//!
//! This module contains the input hook handlers and window polling logic
//! for tracking user activity.

pub mod input_hooks;
pub mod window_poller;

pub use input_hooks::*;
pub use window_poller::*;
