//! # SyCore
//!
//! A pure, delivery-agnostic **domain kernel** for managing musicians who play
//! for various orchestras at shared venues. The kernel holds the complete
//! federation truth and exposes:
//!
//! - [`apply`] — total transitions `(state, command) -> Result<Transition, KernelError>`
//!   that reject hard conflicts, warn on soft ones, and emit [`event::Event`]s.
//! - [`query`] — read-only analyses (`conflicts`, `coverage`, `legal_assignments`).
//! - [`view`] — privacy-preserving per-actor projections (`view_for_musician`,
//!   `view_for_orchestra`, `view_for_venue`). No actor is omniscient.
//!
//! The core is **pure**: no filesystem, network, clock, randomness, or
//! environment access, and no serialization in the public API. JSON loading of
//! the bundled sample data lives behind the opt-in `seed` feature.
//!
//! ## Quick start
//! ```
//! use sycore::apply::apply;
//! use sycore::command::Command;
//! use sycore::ids::{MusicianId, OrchestraId};
//! use sycore::state::Federation;
//!
//! let f = Federation::new();
//! let f = apply(&f, Command::FoundOrchestra {
//!     id: OrchestraId::new("RSO"), name: "Riverside Symphony".into(),
//! }).unwrap().state;
//! let t = apply(&f, Command::RegisterMusician {
//!     id: MusicianId::new("M001"), name: "James".into(),
//!     primary_instrument: "Violin I".into(), availability_pct: 100,
//! }).unwrap();
//! assert_eq!(t.events.len(), 1);
//! ```

pub mod apply;
pub mod command;
pub mod entity;
pub mod error;
pub mod event;
pub mod ids;
pub mod query;
pub mod state;
pub mod time;
pub mod view;

#[cfg(feature = "seed")]
pub mod seed;
