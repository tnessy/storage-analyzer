//! Disk-usage TUI. Module boundaries follow design §3.2.

pub mod app;
pub mod delete;
pub mod diskio;
pub mod eta;
pub mod history;
pub mod layout;
pub mod model;
pub mod scan;
pub mod types;
pub mod ui;

/// Application identifier for XDG paths and config. The only place the name is spelled out.
pub const APP_ID: &str = "storage-analyzer";
