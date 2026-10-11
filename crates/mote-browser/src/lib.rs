//! Isolated headed browser commands for approved local fixtures.
//!
//! The process uses a per-character Mote profile, but the command page lives in
//! a fresh incognito context. Cookies and site sessions intentionally do not
//! persist. No existing browser or default Chrome profile can be attached.
//! Protected requests are aborted and clicks return inert descriptions. There
//! is no approved execution API. Future approval execution must consume one
//! exact intercepted request once within this context. Extra requests stay
//! blocked. Timeout, navigation and worker restart invalidate authorization.
//! A click fingerprint describes prospective DOM/form state, not an observed
//! HTTP request. It cannot authorize releasing a future request on its own.

mod evidence;
mod launch;
mod policy;
mod snapshot;
mod worker;

pub use evidence::{BrowserEvidence, HarEntry};
pub use launch::LaunchConfig;
pub use policy::{ApprovedOrigins, BrowserEffect, BrowserPolicy};
pub use snapshot::{BrowserNode, BrowserSnapshot, NodeRef, SnapshotMap};
pub use worker::{BrowserCommand, BrowserCommitIntent, BrowserHost, BrowserResult, BrowserSession};
