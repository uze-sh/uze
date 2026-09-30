//! How a stored capability reaches a harness.
//!
//! The half of the product that the package model exists to serve.
//! [`integration`] is the contract a harness vertical implements;
//! [`router`] decides which mechanism a capability is compatible with and
//! [`exposure`] plans the concrete artifacts; [`engine`] carries the plan
//! out; [`state`] and [`persistence`] record what was written, as typed
//! receipts; [`reconciliation`] compares that record against what is
//! actually on disk, which is what makes removal safe. [`session`] names a
//! harness's conversation, which the contract speaks; carrying one across a
//! launch is the workspace's, in `uze-workspace`.
//!
//! Vendor-neutral throughout — no module here names a harness. The concrete
//! verticals live in `uze-integrations`, behind
//! [`integration::IntegrationPort`].

pub mod delivered_root;
pub mod engine;
pub mod exposure;
pub mod integration;
pub mod leftovers;
pub mod persistence;
pub mod reconciliation;
pub mod router;
pub mod session;
pub mod state;
