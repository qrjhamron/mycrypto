//! Terminal User Interface module.
//!
//! Single-view CLI-style interface with "/" command system.
//!
//! Layout:
//! - Top bar: mYcrypto [PAPER] | prices | status | time
//! - Main area: current page content
//! - Input bar: styled border with "> _" prompt
//!
//! # Architecture
//!
//! The TUI follows a unidirectional data flow:
//! 1. `App` owns the `AppState` and handles input events
//! 2. Input is parsed into commands or chat messages
//! 3. Commands modify state or navigate pages
//! 4. The render pass reads state and produces widgets
//!
//! Pages are pure rendering functions that take state and produce content.
//! No business logic lives in the rendering code.

pub mod app;
pub mod command;
pub mod pages;
pub mod theme;
pub mod widgets;

// Re-export main entry point
pub use app::run;
