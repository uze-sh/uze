//! Tests for the workspace client.
//!
//! Moved out of `orchestrator.rs` alongside the `render`/`input` split:
//! they were the last ~500 lines standing between a reader and the
//! session-driving code the file is actually about.
//!
//! One module per thing a reader looks for, and `support` for the fixtures
//! they share. A test of one private function lives beside that function.

use super::*;

// Gated one by one although the whole tree is test-only: the architecture
// suite tells a test file from production by the `cfg(test)` on its `mod` line.
#[cfg(test)]
mod activity;
#[cfg(test)]
mod agent_writes;
#[cfg(test)]
mod agents;
#[cfg(test)]
mod approval;
#[cfg(test)]
mod attach;
#[cfg(test)]
mod context_menu;
#[cfg(test)]
mod declared;
#[cfg(test)]
mod declared_actions;
mod declared_events;
mod declared_media;
#[cfg(test)]
mod declared_motion;
#[cfg(test)]
mod declared_notify;
#[cfg(test)]
mod declared_resident;
#[cfg(test)]
mod delivery;
#[cfg(test)]
mod drawing;
#[cfg(test)]
mod extension_keys;
#[cfg(test)]
mod extension_offer;
#[cfg(test)]
mod extensions;
#[cfg(test)]
mod foot;
#[cfg(test)]
mod header;
#[cfg(test)]
mod modal;
#[cfg(test)]
mod overview;
#[cfg(test)]
mod panes;
#[cfg(all(test, feature = "bench"))]
mod perf;
#[cfg(test)]
mod picker;
#[cfg(test)]
mod project_labels;
#[cfg(test)]
mod prompt_buffer;
#[cfg(test)]
mod recent;
#[cfg(test)]
mod selection;
#[cfg(test)]
mod sidebar;
#[cfg(test)]
mod slots;
#[cfg(test)]
mod status;
#[cfg(test)]
mod strip;
#[cfg(test)]
mod support;
#[cfg(test)]
mod surfaces;
#[cfg(test)]
mod switch;
#[cfg(test)]
mod tasks;
#[cfg(test)]
mod timeline;
