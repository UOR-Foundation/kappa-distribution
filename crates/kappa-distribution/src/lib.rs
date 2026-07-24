//! # kappa-distribution - the κ-Distribution protocol standard
//!
//! The universal, backend/transport/consensus-agnostic standard for a content-addressed `/v2/`
//! distribution registry. Any system that upholds these byte contracts and operation semantics is a
//! conforming κ-Distribution registry.
//!
//! **Identity is the κ-label and nothing else.** Mutable tags and DNS-style `org:path@version`
//! human-names are a resolution overlay that never participates in addressing.
//!
//! This crate is **I/O-free**. It defines:
//! - the **canonical byte forms** ([`edge`], [`compose`]) whose hashes must agree across every
//!   registry, or federation of the same content silently diverges;
//! - the **error taxonomy** ([`error`], spec §6.16);
//! - the **conformance levels** ([`level`], spec §13.1).
//!
//! Storage, transport, consensus, and hashing are the implementor's concern.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod compose;
pub mod edge;
pub mod error;
pub mod level;

pub use compose::{compose_canonical, witness_blob, ComposeError, Op};
pub use error::ErrorCode;
pub use level::ConformanceLevel;
