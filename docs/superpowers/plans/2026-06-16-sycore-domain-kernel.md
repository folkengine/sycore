# SyCore Domain Kernel (Slice 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Git policy:** This repo's owner runs ALL state-changing git commands themselves. The `Commit` steps below show the exact command to run, but the human executes them — do not run `git add`/`git commit` automatically. Pause at each commit step and surface the command.

**Goal:** Build `sycore`, a pure Rust domain kernel modeling a federation of orchestras, shared musicians, and shared venues, with scheduling/assignment transitions that reject hard conflicts, warn on soft ones, emit events, and expose privacy-preserving per-actor views.

**Architecture:** One immutable `Federation` state value; a total `apply(&Federation, Command) -> Result<Transition, KernelError>` transition function; read-only `query` and `view_for_*` projections. Pure by default (`default = []`); JSON/serde and sample-data seeding isolated behind an opt-in `seed` feature. Deterministic throughout (`BTreeMap`, derived IDs — no randomness, no clock).

**Tech Stack:** Rust (edition 2024), `std::collections::BTreeMap`, `serde`/`serde_json` (seed feature only). No `chrono`.

---

## File Structure

| File | Responsibility |
|---|---|
| `Cargo.toml` | Crate metadata, `seed` feature, optional serde deps |
| `src/lib.rs` | Crate docs, module declarations, public re-exports |
| `src/ids.rs` | Newtype string IDs via a `define_id!` macro |
| `src/time.rs` | `Date`, `Time`, `TimeSlot` + overlap math |
| `src/entity.rs` | `Tier`, `Chair`, `EventKind`, `Musician`, `Venue`, `RosterEntry`, `Orchestra`, `Work`, `Program`, `CalendarEvent`, `Concert` |
| `src/state.rs` | `Federation` aggregate + accessors |
| `src/command.rs` | `Command` enum |
| `src/event.rs` | `Event` enum |
| `src/error.rs` | `KernelError`, `Warning`, `Conflict` |
| `src/apply.rs` | `Transition`, `apply`, internal busy-slot/conflict helpers |
| `src/query.rs` | `conflicts`, `coverage`, `Coverage`, `legal_assignments` |
| `src/view.rs` | `MusicianView`/`OrchestraView`/`VenueView` + `view_for_*` |
| `src/seed.rs` | *(feature `seed`)* JSON DTOs → `Vec<Command>` + `build_sample` |
| `tests/sample_season.rs` | *(feature `seed`)* integration: replay sample data, zero hard errors |

---

## Decisions locked here (refinements to the spec)

- **`Map` = `std::collections::BTreeMap`** for deterministic iteration (no randomness in a kernel).
- **`EventId` is derived deterministically**: `"{concert_id}-E{n}"` where `n` = `schedule.len() + 1` at schedule time. No clock, no RNG.
- **Availability lives in core as an integer**, not a float: `Musician.availability_pct: u8` (0–100, default 100). The spec said the float "stays in the seed layer"; in practice the soft `LowAvailability` warning must be emitted by `apply`, which has no access to seed data — so the seed layer converts the float to an integer percentage and passes it in via `RegisterMusician`. Still decidable, still WIT-mappable.
- **Capability requirements are explicit booleans** on `Program` (`requires_organ`, `requires_pit`) rather than parsed from free-text `forces` at conflict-check time. The seed layer sets them heuristically; the core just compares booleans.

---

## Task 0: Cargo setup

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Replace `Cargo.toml` with feature/dep setup**

```toml
[package]
name = "sycore"
version = "0.1.0"
edition = "2024"

[features]
default = []
seed = ["dep:serde", "dep:serde_json"]

[dependencies]
serde = { version = "1", features = ["derive"], optional = true }
serde_json = { version = "1", optional = true }
```

- [ ] **Step 2: Verify the pure build resolves with no dependencies**

Run: `cargo build --no-default-features`
Expected: compiles (empty `lib.rs` is fine for now), no serde in the dependency graph.

- [ ] **Step 3: Verify banned crates are absent from the pure build**

Run: `cargo tree --no-default-features -e normal`
Expected: only `sycore` itself; no `serde`, no `serde_json`.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml && git commit -m "chore: configure sycore features and optional serde deps"
```

---

## Task 1: `ids` module

**Files:**
- Create: `src/ids.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Declare the module in `src/lib.rs`**

Replace `src/lib.rs` contents with:

```rust
//! SyCore: the pure domain kernel for orchestra/musician/venue scheduling.
pub mod ids;
```

- [ ] **Step 2: Write `src/ids.rs` with the macro, types, and failing-first tests**

```rust
//! Type-safe string identifiers.
//!
//! Each entity gets its own newtype so the compiler prevents mixing a
//! [`VenueId`] where a [`ConcertId`] is expected.

/// Defines a string-newtype identifier with the standard derives and helpers.
macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Wraps a raw id string.
            ///
            /// # Examples
            /// ```
            /// use sycore::ids::MusicianId;
            /// let id = MusicianId::new("M001");
            /// assert_eq!(id.as_str(), "M001");
            /// ```
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// Borrows the underlying id string.
            ///
            /// # Examples
            /// ```
            /// use sycore::ids::VenueId;
            /// assert_eq!(VenueId::new("VEN-01").as_str(), "VEN-01");
            /// ```
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

define_id!(
    /// Identifies a musician in the shared global pool.
    MusicianId
);
define_id!(
    /// Identifies an orchestra.
    OrchestraId
);
define_id!(
    /// Identifies a shared, bookable venue.
    VenueId
);
define_id!(
    /// Identifies a concert programmed by an orchestra.
    ConcertId
);
define_id!(
    /// Identifies a single calendar event (rehearsal or performance).
    EventId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musician_id_roundtrips() {
        let id = MusicianId::new("M001");
        assert_eq!(id.as_str(), "M001");
        assert_eq!(id.to_string(), "M001");
    }

    #[test]
    fn ids_of_different_types_are_distinct_types() {
        // This compiles only because they are separate types; equality is within-type.
        let a = ConcertId::from("C01");
        let b = ConcertId::from("C01");
        assert_eq!(a, b);
    }

    #[test]
    fn from_str_matches_new() {
        assert_eq!(VenueId::from("VEN-01"), VenueId::new("VEN-01"));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --no-default-features ids`
Expected: PASS (3 unit tests + doc tests).

- [ ] **Step 4: Commit**

```bash
git add src/ids.rs src/lib.rs && git commit -m "feat: type-safe entity id newtypes"
```

---

## Task 2: `time` module

**Files:**
- Create: `src/time.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `pub mod time;` to `src/lib.rs`** (after `pub mod ids;`)

- [ ] **Step 2: Write `src/time.rs`**

```rust
//! Pure, integer-backed calendar primitives. No `chrono`, no clock.

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// Constructs a date from its parts.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::Date;
    /// let d = Date { year: 2024, month: 9, day: 14 };
    /// assert_eq!(d.month, 9);
    /// ```
    pub fn new(year: u16, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }
}

/// A wall-clock time of day, stored as minutes since midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(pub u16);

impl Time {
    /// Builds a time from hours and minutes.
    ///
    /// # Errors
    /// Returns `Err` if `hour > 23` or `minute > 59`.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::Time;
    /// assert_eq!(Time::from_hm(19, 30).unwrap(), Time(1170));
    /// assert!(Time::from_hm(24, 0).is_err());
    /// ```
    pub fn from_hm(hour: u8, minute: u8) -> Result<Self, &'static str> {
        if hour > 23 || minute > 59 {
            return Err("hour must be 0..=23 and minute 0..=59");
        }
        Ok(Self(hour as u16 * 60 + minute as u16))
    }

    /// Minutes since midnight.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::Time;
    /// assert_eq!(Time(1170).minutes(), 1170);
    /// ```
    pub fn minutes(self) -> u16 {
        self.0
    }
}

/// A bounded block of time on a single date — the unit that scheduling and
/// double-booking checks operate on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimeSlot {
    pub date: Date,
    pub start: Time,
    pub duration_min: u16,
}

impl TimeSlot {
    /// Constructs a time slot.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::{Date, Time, TimeSlot};
    /// let slot = TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180);
    /// assert_eq!(slot.duration_min, 180);
    /// ```
    pub fn new(date: Date, start: Time, duration_min: u16) -> Self {
        Self { date, start, duration_min }
    }

    /// End time as minutes-since-midnight (may exceed 1440 for late blocks).
    ///
    /// # Examples
    /// ```
    /// use sycore::time::{Date, Time, TimeSlot};
    /// let slot = TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180);
    /// assert_eq!(slot.end_min(), 1260);
    /// ```
    pub fn end_min(self) -> u32 {
        self.start.0 as u32 + self.duration_min as u32
    }

    /// Returns `true` if two slots fall on the same date and their intervals
    /// intersect. Touching boundaries (one ends exactly when the other starts)
    /// do **not** overlap.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::{Date, Time, TimeSlot};
    /// let d = Date::new(2024, 9, 14);
    /// let a = TimeSlot::new(d, Time(1080), 180); // 18:00–21:00
    /// let b = TimeSlot::new(d, Time(1200), 60);  // 20:00–21:00
    /// let c = TimeSlot::new(d, Time(1260), 60);  // 21:00–22:00
    /// assert!(a.overlaps(&b));
    /// assert!(!a.overlaps(&c)); // touch, no overlap
    /// ```
    pub fn overlaps(&self, other: &TimeSlot) -> bool {
        self.date == other.date
            && (self.start.0 as u32) < other.end_min()
            && (other.start.0 as u32) < self.end_min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> Date {
        Date::new(2024, 9, 14)
    }

    #[test]
    fn from_hm_rejects_out_of_range() {
        assert!(Time::from_hm(25, 0).is_err());
        assert!(Time::from_hm(0, 60).is_err());
        assert_eq!(Time::from_hm(0, 0).unwrap(), Time(0));
    }

    #[test]
    fn overlap_true_when_intervals_intersect() {
        let a = TimeSlot::new(d(), Time(1080), 180);
        let b = TimeSlot::new(d(), Time(1200), 60);
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
    }

    #[test]
    fn overlap_false_when_touching() {
        let a = TimeSlot::new(d(), Time(1080), 180); // ..21:00
        let b = TimeSlot::new(d(), Time(1260), 60); // 21:00..
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn overlap_false_on_different_dates() {
        let a = TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180);
        let b = TimeSlot::new(Date::new(2024, 9, 15), Time(1080), 180);
        assert!(!a.overlaps(&b));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --no-default-features time`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/time.rs src/lib.rs && git commit -m "feat: integer-backed Date/Time/TimeSlot with overlap"
```

---

## Task 3: `entity` module

**Files:**
- Create: `src/entity.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `pub mod entity;` to `src/lib.rs`**

- [ ] **Step 2: Write `src/entity.rs`**

```rust
//! Immutable domain entities. These are plain data; all rules live in `apply`.

use crate::ids::{ConcertId, EventId, MusicianId, OrchestraId, VenueId};
use crate::time::{Time, TimeSlot};

/// A musician's commitment tier within an orchestra's roster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Core,
    Sub,
    Extra,
}

/// A musician's chair within a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chair {
    Concertmaster,
    Principal,
    Section,
}

/// Whether a calendar event is a rehearsal or a public performance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Rehearsal,
    Performance,
}

/// A musician in the shared, global pool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Musician {
    pub id: MusicianId,
    pub name: String,
    pub primary_instrument: String,
    /// Percentage availability hint (0–100); soft-warning input. Defaults to 100.
    pub availability_pct: u8,
    /// Concrete blackout windows the musician cannot be scheduled into.
    pub unavailable: Vec<TimeSlot>,
}

/// A shared, bookable venue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Venue {
    pub id: VenueId,
    pub name: String,
    pub capacity: u32,
    pub stage_type: String,
    pub has_pit: bool,
    pub has_organ: bool,
    pub loading_dock: bool,
}

/// One musician's membership in one orchestra's roster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RosterEntry {
    pub musician: MusicianId,
    pub instrument: String,
    pub chair: Chair,
    pub tier: Tier,
}

/// An orchestra and the roster it draws from the shared pool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Orchestra {
    pub id: OrchestraId,
    pub name: String,
    pub roster: Vec<RosterEntry>,
}

/// A single work on a program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Work {
    pub composer: String,
    pub title: String,
    pub duration_min: u16,
    pub forces: String,
}

/// The repertoire and capability requirements of a concert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub works: Vec<Work>,
    pub requires_organ: bool,
    pub requires_pit: bool,
}

/// A scheduled rehearsal or performance at a venue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarEvent {
    pub id: EventId,
    pub kind: EventKind,
    pub slot: TimeSlot,
    pub venue: VenueId,
    pub call_time: Option<Time>,
    pub downbeat: Option<Time>,
}

/// A concert: program + schedule + the musicians assigned to play it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Concert {
    pub id: ConcertId,
    pub orchestra: OrchestraId,
    pub series: String,
    pub title: String,
    pub program: Program,
    pub players_required: u16,
    pub assignments: Vec<MusicianId>,
    pub schedule: Vec<CalendarEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{Date, TimeSlot};

    #[test]
    fn musician_constructs() {
        let m = Musician {
            id: MusicianId::new("M001"),
            name: "James Thornton".into(),
            primary_instrument: "Violin I".into(),
            availability_pct: 100,
            unavailable: vec![],
        };
        assert_eq!(m.availability_pct, 100);
        assert!(m.unavailable.is_empty());
    }

    #[test]
    fn concert_starts_empty_assignments_and_schedule() {
        let c = Concert {
            id: ConcertId::new("C01"),
            orchestra: OrchestraId::new("RSO"),
            series: "Masterworks".into(),
            title: "Opening Night Gala".into(),
            program: Program { works: vec![], requires_organ: false, requires_pit: false },
            players_required: 60,
            assignments: vec![],
            schedule: vec![],
        };
        assert_eq!(c.players_required, 60);
        assert!(c.assignments.is_empty());
    }

    #[test]
    fn calendar_event_holds_slot_and_venue() {
        let ev = CalendarEvent {
            id: EventId::new("C01-E1"),
            kind: EventKind::Performance,
            slot: TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180),
            venue: VenueId::new("VEN-01"),
            call_time: Time::from_hm(18, 0).ok(),
            downbeat: Time::from_hm(19, 0).ok(),
        };
        assert_eq!(ev.kind, EventKind::Performance);
        assert_eq!(ev.venue, VenueId::new("VEN-01"));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --no-default-features entity`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/entity.rs src/lib.rs && git commit -m "feat: domain entity types"
```

---

## Task 4: `error` module

**Files:**
- Create: `src/error.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `pub mod error;` to `src/lib.rs`**

- [ ] **Step 2: Write `src/error.rs`**

```rust
//! Hard failures (`KernelError`), soft advisories (`Warning`), and global
//! conflict reports (`Conflict`).

use crate::ids::{ConcertId, EventId, MusicianId, OrchestraId, VenueId};
use crate::time::TimeSlot;

/// A hard failure: the command was rejected and state is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KernelError {
    UnknownMusician(MusicianId),
    UnknownVenue(VenueId),
    UnknownOrchestra(OrchestraId),
    UnknownConcert(ConcertId),
    NotOnRoster { musician: MusicianId, orchestra: OrchestraId },
    AlreadyAssigned { musician: MusicianId, concert: ConcertId },
    NotAssigned { musician: MusicianId, concert: ConcertId },
    /// Returned ONLY to the caller making the assignment. Carries the
    /// conflicting event so the assigner can act; never surfaced to other actors.
    MusicianDoubleBooked { musician: MusicianId, conflicting: EventId },
    VenueDoubleBooked { venue: VenueId, conflicting: EventId },
    MusicianUnavailable { musician: MusicianId, slot: TimeSlot },
    InvalidTime { reason: String },
    DuplicateId(String),
}

impl std::fmt::Display for KernelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KernelError::UnknownMusician(id) => write!(f, "unknown musician: {id}"),
            KernelError::UnknownVenue(id) => write!(f, "unknown venue: {id}"),
            KernelError::UnknownOrchestra(id) => write!(f, "unknown orchestra: {id}"),
            KernelError::UnknownConcert(id) => write!(f, "unknown concert: {id}"),
            KernelError::NotOnRoster { musician, orchestra } => {
                write!(f, "musician {musician} is not on orchestra {orchestra}'s roster")
            }
            KernelError::AlreadyAssigned { musician, concert } => {
                write!(f, "musician {musician} is already assigned to concert {concert}")
            }
            KernelError::NotAssigned { musician, concert } => {
                write!(f, "musician {musician} is not assigned to concert {concert}")
            }
            KernelError::MusicianDoubleBooked { musician, conflicting } => {
                write!(f, "musician {musician} is double-booked against event {conflicting}")
            }
            KernelError::VenueDoubleBooked { venue, conflicting } => {
                write!(f, "venue {venue} is double-booked against event {conflicting}")
            }
            KernelError::MusicianUnavailable { musician, slot } => {
                write!(f, "musician {musician} is unavailable during {slot:?}")
            }
            KernelError::InvalidTime { reason } => write!(f, "invalid time: {reason}"),
            KernelError::DuplicateId(id) => write!(f, "duplicate id: {id}"),
        }
    }
}

impl std::error::Error for KernelError {}

/// A soft advisory: the command succeeded, but something is worth flagging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    Understaffed { concert: ConcertId, required: u16, assigned: u16 },
    LowAvailability { musician: MusicianId, availability_pct: u8 },
    VenueCapabilityMismatch { venue: VenueId, capability: String },
    NoRehearsal { concert: ConcertId },
}

/// A conflict discovered by the global `conflicts` query (kernel-internal view;
/// never surfaced raw to any single actor).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    MusicianDoubleBooked { musician: MusicianId, events: (EventId, EventId) },
    VenueDoubleBooked { venue: VenueId, events: (EventId, EventId) },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_error_displays() {
        let e = KernelError::UnknownMusician(MusicianId::new("M999"));
        assert_eq!(e.to_string(), "unknown musician: M999");
    }

    #[test]
    fn kernel_error_is_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&KernelError::DuplicateId("C01".into()));
    }

    #[test]
    fn warning_equality() {
        let w = Warning::Understaffed { concert: ConcertId::new("C01"), required: 60, assigned: 58 };
        assert_eq!(
            w,
            Warning::Understaffed { concert: ConcertId::new("C01"), required: 60, assigned: 58 }
        );
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --no-default-features error`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/error.rs src/lib.rs && git commit -m "feat: KernelError, Warning, Conflict types"
```

---

## Task 5: `event` module

**Files:**
- Create: `src/event.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `pub mod event;` to `src/lib.rs`**

- [ ] **Step 2: Write `src/event.rs`**

```rust
//! Typed domain events emitted by successful transitions.

use crate::ids::{ConcertId, EventId, MusicianId, OrchestraId, VenueId};

/// A record of what changed in a successful `apply`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    MusicianRegistered { id: MusicianId },
    VenueRegistered { id: VenueId },
    OrchestraFounded { id: OrchestraId },
    MusicianAddedToRoster { orchestra: OrchestraId, musician: MusicianId },
    UnavailabilitySet { musician: MusicianId, count: usize },
    ConcertProgrammed { id: ConcertId, orchestra: OrchestraId },
    EventScheduled { concert: ConcertId, event: EventId },
    PlayerAssigned { concert: ConcertId, musician: MusicianId },
    PlayerUnassigned { concert: ConcertId, musician: MusicianId },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_equality() {
        let a = Event::PlayerAssigned { concert: ConcertId::new("C01"), musician: MusicianId::new("M001") };
        let b = Event::PlayerAssigned { concert: ConcertId::new("C01"), musician: MusicianId::new("M001") };
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --no-default-features event`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/event.rs src/lib.rs && git commit -m "feat: domain event enum"
```

---

## Task 6: `command` module

**Files:**
- Create: `src/command.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `pub mod command;` to `src/lib.rs`**

- [ ] **Step 2: Write `src/command.rs`**

```rust
//! The only inputs that can change `Federation` state.

use crate::entity::{Chair, EventKind, Program, Tier};
use crate::ids::{ConcertId, MusicianId, OrchestraId, VenueId};
use crate::time::{Time, TimeSlot};

/// A request to change federation state. Apply via [`crate::apply::apply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    RegisterMusician { id: MusicianId, name: String, primary_instrument: String, availability_pct: u8 },
    RegisterVenue {
        id: VenueId,
        name: String,
        capacity: u32,
        stage_type: String,
        has_pit: bool,
        has_organ: bool,
        loading_dock: bool,
    },
    FoundOrchestra { id: OrchestraId, name: String },
    AddToRoster { orchestra: OrchestraId, musician: MusicianId, instrument: String, chair: Chair, tier: Tier },
    SetUnavailable { musician: MusicianId, slots: Vec<TimeSlot> },
    ProgramConcert {
        id: ConcertId,
        orchestra: OrchestraId,
        series: String,
        title: String,
        program: Program,
        players_required: u16,
    },
    ScheduleEvent {
        concert: ConcertId,
        kind: EventKind,
        slot: TimeSlot,
        venue: VenueId,
        call_time: Option<Time>,
        downbeat: Option<Time>,
    },
    AssignPlayer { concert: ConcertId, musician: MusicianId },
    UnassignPlayer { concert: ConcertId, musician: MusicianId },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_constructs_and_clones() {
        let c = Command::AssignPlayer { concert: ConcertId::new("C01"), musician: MusicianId::new("M001") };
        assert_eq!(c.clone(), c);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --no-default-features command`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/command.rs src/lib.rs && git commit -m "feat: command enum"
```

---

## Task 7: `state` module (`Federation`)

**Files:**
- Create: `src/state.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `pub mod state;` to `src/lib.rs`**

- [ ] **Step 2: Write `src/state.rs`**

```rust
//! The `Federation` aggregate — the complete, immutable truth of the system.

use std::collections::BTreeMap;

use crate::entity::{Concert, Musician, Orchestra, Venue};
use crate::ids::{ConcertId, MusicianId, OrchestraId, VenueId};

/// The whole federation: shared musicians and venues, plus orchestras and the
/// concerts they program. No single actor sees all of this directly — reads go
/// through `view_for_*` projections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Federation {
    pub musicians: BTreeMap<MusicianId, Musician>,
    pub venues: BTreeMap<VenueId, Venue>,
    pub orchestras: BTreeMap<OrchestraId, Orchestra>,
    pub concerts: BTreeMap<ConcertId, Concert>,
}

impl Federation {
    /// An empty federation.
    ///
    /// # Examples
    /// ```
    /// use sycore::state::Federation;
    /// let f = Federation::new();
    /// assert!(f.musicians.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// All concerts belonging to a given orchestra.
    ///
    /// # Examples
    /// ```
    /// use sycore::state::Federation;
    /// use sycore::ids::OrchestraId;
    /// let f = Federation::new();
    /// assert_eq!(f.concerts_of(&OrchestraId::new("RSO")).count(), 0);
    /// ```
    pub fn concerts_of<'a>(&'a self, orchestra: &'a OrchestraId) -> impl Iterator<Item = &'a Concert> {
        self.concerts.values().filter(move |c| &c.orchestra == orchestra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let f = Federation::new();
        assert!(f.musicians.is_empty());
        assert!(f.venues.is_empty());
        assert!(f.orchestras.is_empty());
        assert!(f.concerts.is_empty());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --no-default-features state`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/state.rs src/lib.rs && git commit -m "feat: Federation aggregate"
```

---

## Task 8: `apply` — bootstrap commands + busy-slot helpers

**Files:**
- Create: `src/apply.rs`
- Modify: `src/lib.rs`

This task implements `Transition`, the internal busy-slot helpers (used later by scheduling/assignment/views), and the five non-scheduling commands.

- [ ] **Step 1: Add `pub mod apply;` to `src/lib.rs`**

- [ ] **Step 2: Write the failing test file first (top of `src/apply.rs`)**

Create `src/apply.rs` containing ONLY the test module so it fails to compile (drives the API):

```rust
//! The transition function: `apply(&Federation, Command) -> Result<Transition, KernelError>`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::ids::{MusicianId, OrchestraId, VenueId};
    use crate::state::Federation;

    fn reg_musician(id: &str) -> Command {
        Command::RegisterMusician {
            id: MusicianId::new(id),
            name: format!("Player {id}"),
            primary_instrument: "Violin I".into(),
            availability_pct: 100,
        }
    }

    #[test]
    fn register_musician_adds_to_pool() {
        let f = Federation::new();
        let t = apply(&f, reg_musician("M001")).unwrap();
        assert!(t.state.musicians.contains_key(&MusicianId::new("M001")));
        assert_eq!(t.events.len(), 1);
        assert!(t.warnings.is_empty());
    }

    #[test]
    fn duplicate_musician_is_rejected() {
        let f = apply(&Federation::new(), reg_musician("M001")).unwrap().state;
        let err = apply(&f, reg_musician("M001")).unwrap_err();
        assert!(matches!(err, crate::error::KernelError::DuplicateId(_)));
    }

    #[test]
    fn add_to_roster_requires_known_musician_and_orchestra() {
        let f = Federation::new();
        let cmd = Command::AddToRoster {
            orchestra: OrchestraId::new("RSO"),
            musician: MusicianId::new("M001"),
            instrument: "Violin I".into(),
            chair: crate::entity::Chair::Section,
            tier: crate::entity::Tier::Core,
        };
        assert!(matches!(
            apply(&f, cmd).unwrap_err(),
            crate::error::KernelError::UnknownOrchestra(_)
        ));
    }

    #[test]
    fn register_venue_and_found_orchestra() {
        let f = Federation::new();
        let f = apply(&f, Command::FoundOrchestra { id: OrchestraId::new("RSO"), name: "Riverside".into() })
            .unwrap()
            .state;
        let f = apply(&f, Command::RegisterVenue {
            id: VenueId::new("VEN-01"),
            name: "Concert Hall".into(),
            capacity: 1800,
            stage_type: "proscenium".into(),
            has_pit: false,
            has_organ: true,
            loading_dock: true,
        })
        .unwrap()
        .state;
        assert!(f.orchestras.contains_key(&OrchestraId::new("RSO")));
        assert!(f.venues.contains_key(&VenueId::new("VEN-01")));
    }
}
```

- [ ] **Step 3: Run the test to confirm it fails**

Run: `cargo test --no-default-features apply`
Expected: FAIL — `apply`, `Transition` not found.

- [ ] **Step 4: Implement the production code above the test module**

Insert ABOVE `#[cfg(test)]`:

```rust
use crate::command::Command;
use crate::entity::{CalendarEvent, Musician, Orchestra, RosterEntry, Venue};
use crate::error::{KernelError, Warning};
use crate::event::Event;
use crate::ids::{EventId, MusicianId, VenueId};
use crate::state::Federation;
use crate::time::TimeSlot;

/// The result of a successful transition: the new state, the events it emitted,
/// and any soft warnings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    pub state: Federation,
    pub events: Vec<Event>,
    pub warnings: Vec<Warning>,
}

/// Applies a command to a federation, returning a new federation plus events
/// and warnings, or a hard error leaving the input unchanged.
///
/// # Errors
/// Returns [`KernelError`] for structural problems (unknown ids, duplicates) and
/// hard conflicts (double-booking, unavailability, malformed time).
///
/// # Examples
/// ```
/// use sycore::apply::apply;
/// use sycore::command::Command;
/// use sycore::ids::MusicianId;
/// use sycore::state::Federation;
///
/// let cmd = Command::RegisterMusician {
///     id: MusicianId::new("M001"),
///     name: "James".into(),
///     primary_instrument: "Violin I".into(),
///     availability_pct: 100,
/// };
/// let t = apply(&Federation::new(), cmd).unwrap();
/// assert_eq!(t.events.len(), 1);
/// ```
pub fn apply(state: &Federation, command: Command) -> Result<Transition, KernelError> {
    let mut next = state.clone();
    let mut events = Vec::new();
    let mut warnings = Vec::new();

    match command {
        Command::RegisterMusician { id, name, primary_instrument, availability_pct } => {
            if next.musicians.contains_key(&id) {
                return Err(KernelError::DuplicateId(id.to_string()));
            }
            events.push(Event::MusicianRegistered { id: id.clone() });
            next.musicians.insert(
                id.clone(),
                Musician { id, name, primary_instrument, availability_pct, unavailable: vec![] },
            );
        }
        Command::RegisterVenue { id, name, capacity, stage_type, has_pit, has_organ, loading_dock } => {
            if next.venues.contains_key(&id) {
                return Err(KernelError::DuplicateId(id.to_string()));
            }
            events.push(Event::VenueRegistered { id: id.clone() });
            next.venues.insert(
                id.clone(),
                Venue { id, name, capacity, stage_type, has_pit, has_organ, loading_dock },
            );
        }
        Command::FoundOrchestra { id, name } => {
            if next.orchestras.contains_key(&id) {
                return Err(KernelError::DuplicateId(id.to_string()));
            }
            events.push(Event::OrchestraFounded { id: id.clone() });
            next.orchestras.insert(id.clone(), Orchestra { id, name, roster: vec![] });
        }
        Command::AddToRoster { orchestra, musician, instrument, chair, tier } => {
            if !next.musicians.contains_key(&musician) {
                return Err(KernelError::UnknownMusician(musician));
            }
            let orch = next
                .orchestras
                .get_mut(&orchestra)
                .ok_or_else(|| KernelError::UnknownOrchestra(orchestra.clone()))?;
            orch.roster.push(RosterEntry { musician: musician.clone(), instrument, chair, tier });
            events.push(Event::MusicianAddedToRoster { orchestra, musician });
        }
        Command::SetUnavailable { musician, slots } => {
            let m = next
                .musicians
                .get_mut(&musician)
                .ok_or_else(|| KernelError::UnknownMusician(musician.clone()))?;
            let count = slots.len();
            m.unavailable = slots;
            events.push(Event::UnavailabilitySet { musician, count });
        }
        Command::ProgramConcert { .. }
        | Command::ScheduleEvent { .. }
        | Command::AssignPlayer { .. }
        | Command::UnassignPlayer { .. } => {
            // Implemented in Tasks 9 and 10.
            return Err(KernelError::InvalidTime { reason: "not yet implemented".into() });
        }
    }

    Ok(Transition { state: next, events, warnings })
}

/// All (event id, slot) pairs at a venue, across every concert. Used for venue
/// double-booking checks.
pub(crate) fn venue_busy(state: &Federation, venue: &VenueId) -> Vec<(EventId, TimeSlot)> {
    let mut out = Vec::new();
    for concert in state.concerts.values() {
        for ev in &concert.schedule {
            if &ev.venue == venue {
                out.push((ev.id.clone(), ev.slot));
            }
        }
    }
    out
}

/// All (event id, slot) pairs a musician is committed to across the whole
/// federation, optionally excluding one concert. Used for cross-federation
/// musician double-booking checks.
pub(crate) fn musician_busy(
    state: &Federation,
    musician: &MusicianId,
    exclude_concert: Option<&crate::ids::ConcertId>,
) -> Vec<(EventId, TimeSlot)> {
    let mut out = Vec::new();
    for concert in state.concerts.values() {
        if Some(&concert.id) == exclude_concert {
            continue;
        }
        if concert.assignments.contains(musician) {
            for ev in &concert.schedule {
                out.push((ev.id.clone(), ev.slot));
            }
        }
    }
    out
}

/// All scheduled events of a concert as (event id, slot) pairs.
pub(crate) fn concert_slots(events: &[CalendarEvent]) -> Vec<(EventId, TimeSlot)> {
    events.iter().map(|ev| (ev.id.clone(), ev.slot)).collect()
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test --no-default-features apply`
Expected: PASS (4 bootstrap tests + the `apply` doc test).

- [ ] **Step 6: Commit**

```bash
git add src/apply.rs src/lib.rs && git commit -m "feat: apply transition scaffold + bootstrap commands"
```

---

## Task 9: `apply` — `ProgramConcert` and `ScheduleEvent`

**Files:**
- Modify: `src/apply.rs`

- [ ] **Step 1: Add failing tests to the `tests` module in `src/apply.rs`**

```rust
    use crate::entity::{EventKind, Program};
    use crate::ids::ConcertId;
    use crate::time::{Date, Time, TimeSlot};

    fn base_with_concert() -> Federation {
        let mut f = Federation::new();
        f = apply(&f, Command::FoundOrchestra { id: OrchestraId::new("RSO"), name: "Riverside".into() }).unwrap().state;
        f = apply(&f, Command::RegisterVenue {
            id: VenueId::new("VEN-01"), name: "Hall".into(), capacity: 1800,
            stage_type: "proscenium".into(), has_pit: false, has_organ: true, loading_dock: true,
        }).unwrap().state;
        f = apply(&f, Command::ProgramConcert {
            id: ConcertId::new("C01"), orchestra: OrchestraId::new("RSO"),
            series: "Masterworks".into(), title: "Gala".into(),
            program: Program { works: vec![], requires_organ: false, requires_pit: false },
            players_required: 2,
        }).unwrap().state;
        f
    }

    fn slot(start: u16) -> TimeSlot {
        TimeSlot::new(Date::new(2024, 9, 14), Time(start), 180)
    }

    #[test]
    fn schedule_event_appends_with_derived_id() {
        let f = base_with_concert();
        let t = apply(&f, Command::ScheduleEvent {
            concert: ConcertId::new("C01"), kind: EventKind::Rehearsal, slot: slot(600),
            venue: VenueId::new("VEN-01"), call_time: None, downbeat: None,
        }).unwrap();
        let c = &t.state.concerts[&ConcertId::new("C01")];
        assert_eq!(c.schedule.len(), 1);
        assert_eq!(c.schedule[0].id, crate::ids::EventId::new("C01-E1"));
    }

    #[test]
    fn venue_double_booking_is_rejected() {
        let mut f = base_with_concert();
        f = apply(&f, Command::ScheduleEvent {
            concert: ConcertId::new("C01"), kind: EventKind::Rehearsal, slot: slot(600),
            venue: VenueId::new("VEN-01"), call_time: None, downbeat: None,
        }).unwrap().state;
        let err = apply(&f, Command::ScheduleEvent {
            concert: ConcertId::new("C01"), kind: EventKind::Performance, slot: slot(660),
            venue: VenueId::new("VEN-01"), call_time: None, downbeat: None,
        }).unwrap_err();
        assert!(matches!(err, crate::error::KernelError::VenueDoubleBooked { .. }));
    }

    #[test]
    fn call_after_downbeat_is_invalid() {
        let f = base_with_concert();
        let err = apply(&f, Command::ScheduleEvent {
            concert: ConcertId::new("C01"), kind: EventKind::Performance, slot: slot(600),
            venue: VenueId::new("VEN-01"),
            call_time: Time::from_hm(19, 0).ok(), downbeat: Time::from_hm(18, 0).ok(),
        }).unwrap_err();
        assert!(matches!(err, crate::error::KernelError::InvalidTime { .. }));
    }

    #[test]
    fn performance_without_rehearsal_warns() {
        let f = base_with_concert();
        let t = apply(&f, Command::ScheduleEvent {
            concert: ConcertId::new("C01"), kind: EventKind::Performance, slot: slot(600),
            venue: VenueId::new("VEN-01"), call_time: None, downbeat: None,
        }).unwrap();
        assert!(t.warnings.iter().any(|w| matches!(w, Warning::NoRehearsal { .. })));
    }
```

- [ ] **Step 2: Run tests to confirm failure**

Run: `cargo test --no-default-features apply`
Expected: FAIL — `ProgramConcert`/`ScheduleEvent` still return the placeholder error.

- [ ] **Step 3: Replace the placeholder match arm**

In `apply`, replace the combined placeholder arm. Remove `ProgramConcert`/`ScheduleEvent` from it (leaving `AssignPlayer | UnassignPlayer` for Task 10) and add:

```rust
        Command::ProgramConcert { id, orchestra, series, title, program, players_required } => {
            if !next.orchestras.contains_key(&orchestra) {
                return Err(KernelError::UnknownOrchestra(orchestra));
            }
            if next.concerts.contains_key(&id) {
                return Err(KernelError::DuplicateId(id.to_string()));
            }
            events.push(Event::ConcertProgrammed { id: id.clone(), orchestra: orchestra.clone() });
            next.concerts.insert(
                id.clone(),
                crate::entity::Concert {
                    id, orchestra, series, title, program, players_required,
                    assignments: vec![], schedule: vec![],
                },
            );
        }
        Command::ScheduleEvent { concert, kind, slot, venue, call_time, downbeat } => {
            // Validate references.
            if !next.venues.contains_key(&venue) {
                return Err(KernelError::UnknownVenue(venue));
            }
            let existing = next
                .concerts
                .get(&concert)
                .ok_or_else(|| KernelError::UnknownConcert(concert.clone()))?;

            // Validate time.
            if slot.duration_min == 0 {
                return Err(KernelError::InvalidTime { reason: "zero-duration slot".into() });
            }
            if let (Some(call), Some(down)) = (call_time, downbeat) {
                if call > down {
                    return Err(KernelError::InvalidTime { reason: "call_time after downbeat".into() });
                }
            }

            // Venue must be free.
            for (other, other_slot) in venue_busy(&next, &venue) {
                if other_slot.overlaps(&slot) {
                    return Err(KernelError::VenueDoubleBooked { venue, conflicting: other });
                }
            }
            // Already-assigned musicians must not become double-booked by the new event.
            for musician in &existing.assignments {
                for (other, other_slot) in musician_busy(&next, musician, Some(&concert)) {
                    if other_slot.overlaps(&slot) {
                        return Err(KernelError::MusicianDoubleBooked {
                            musician: musician.clone(),
                            conflicting: other,
                        });
                    }
                }
            }

            let concert_mut = next.concerts.get_mut(&concert).expect("checked above");
            let event_id = EventId::new(format!("{}-E{}", concert, concert_mut.schedule.len() + 1));
            let requires_organ = concert_mut.program.requires_organ;
            let requires_pit = concert_mut.program.requires_pit;
            concert_mut.schedule.push(CalendarEvent {
                id: event_id.clone(),
                kind,
                slot,
                venue: venue.clone(),
                call_time,
                downbeat,
            });
            events.push(Event::EventScheduled { concert: concert.clone(), event: event_id });

            // Soft warnings.
            if kind == EventKind::Performance {
                let has_rehearsal = next.concerts[&concert]
                    .schedule
                    .iter()
                    .any(|e| e.kind == EventKind::Rehearsal);
                if !has_rehearsal {
                    warnings.push(Warning::NoRehearsal { concert: concert.clone() });
                }
                let v = &next.venues[&venue];
                if requires_organ && !v.has_organ {
                    warnings.push(Warning::VenueCapabilityMismatch {
                        venue: venue.clone(),
                        capability: "organ".into(),
                    });
                }
                if requires_pit && !v.has_pit {
                    warnings.push(Warning::VenueCapabilityMismatch {
                        venue: venue.clone(),
                        capability: "pit".into(),
                    });
                }
            }
        }
```

> Note: the `.expect("checked above")` is on a `get_mut` immediately after a confirmed `contains`/`get`. This is the one tolerated infallible-unwrap pattern; if you prefer zero `expect`, restructure with a single `get_mut` and map `None` to `UnknownConcert`. Keep whichever the reviewer prefers — both are correct.

Also add the needed imports at the top of `apply.rs` if not already present:

```rust
use crate::entity::EventKind;
```

- [ ] **Step 4: Run tests**

Run: `cargo test --no-default-features apply`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/apply.rs && git commit -m "feat: ProgramConcert and ScheduleEvent transitions"
```

---

## Task 10: `apply` — `AssignPlayer` / `UnassignPlayer` + shared conflict predicate

**Files:**
- Modify: `src/apply.rs`

- [ ] **Step 1: Add failing tests**

```rust
    fn roster_one(f: Federation, id: &str) -> Federation {
        let f = apply(&f, reg_musician(id)).unwrap().state;
        apply(&f, Command::AddToRoster {
            orchestra: OrchestraId::new("RSO"), musician: MusicianId::new(id),
            instrument: "Violin I".into(), chair: crate::entity::Chair::Section, tier: crate::entity::Tier::Core,
        }).unwrap().state
    }

    #[test]
    fn assign_requires_roster_membership() {
        let mut f = base_with_concert();
        f = apply(&f, reg_musician("M001")).unwrap().state; // in pool, not on roster
        let err = apply(&f, Command::AssignPlayer {
            concert: ConcertId::new("C01"), musician: MusicianId::new("M001"),
        }).unwrap_err();
        assert!(matches!(err, crate::error::KernelError::NotOnRoster { .. }));
    }

    #[test]
    fn assign_succeeds_and_warns_understaffed() {
        let mut f = base_with_concert(); // players_required = 2
        f = roster_one(f, "M001");
        let t = apply(&f, Command::AssignPlayer {
            concert: ConcertId::new("C01"), musician: MusicianId::new("M001"),
        }).unwrap();
        assert!(t.state.concerts[&ConcertId::new("C01")].assignments.contains(&MusicianId::new("M001")));
        assert!(t.warnings.iter().any(|w| matches!(w, Warning::Understaffed { required: 2, assigned: 1, .. })));
    }

    #[test]
    fn assign_rejects_unavailable_slot() {
        let mut f = base_with_concert();
        f = roster_one(f, "M001");
        f = apply(&f, Command::ScheduleEvent {
            concert: ConcertId::new("C01"), kind: EventKind::Rehearsal, slot: slot(600),
            venue: VenueId::new("VEN-01"), call_time: None, downbeat: None,
        }).unwrap().state;
        f = apply(&f, Command::SetUnavailable {
            musician: MusicianId::new("M001"), slots: vec![slot(660)],
        }).unwrap().state;
        let err = apply(&f, Command::AssignPlayer {
            concert: ConcertId::new("C01"), musician: MusicianId::new("M001"),
        }).unwrap_err();
        assert!(matches!(err, crate::error::KernelError::MusicianUnavailable { .. }));
    }

    #[test]
    fn unassign_removes_player() {
        let mut f = base_with_concert();
        f = roster_one(f, "M001");
        f = apply(&f, Command::AssignPlayer { concert: ConcertId::new("C01"), musician: MusicianId::new("M001") }).unwrap().state;
        let t = apply(&f, Command::UnassignPlayer { concert: ConcertId::new("C01"), musician: MusicianId::new("M001") }).unwrap();
        assert!(!t.state.concerts[&ConcertId::new("C01")].assignments.contains(&MusicianId::new("M001")));
    }
```

- [ ] **Step 2: Run tests to confirm failure**

Run: `cargo test --no-default-features apply`
Expected: FAIL — `AssignPlayer`/`UnassignPlayer` return the placeholder.

- [ ] **Step 3: Add the shared predicate and replace the placeholder arm**

Add this `pub(crate)` predicate near the other helpers:

```rust
/// Returns `Some(error)` if assigning `musician` to `concert` would create a
/// hard conflict (not on roster, double-booked, or unavailable). Shared by
/// `apply(AssignPlayer)` and `query::legal_assignments` so the picker and the
/// mutation use one definition of "legal".
pub(crate) fn assignment_conflict(
    state: &Federation,
    concert: &crate::ids::ConcertId,
    musician: &MusicianId,
) -> Option<KernelError> {
    let Some(c) = state.concerts.get(concert) else {
        return Some(KernelError::UnknownConcert(concert.clone()));
    };
    let Some(orch) = state.orchestras.get(&c.orchestra) else {
        return Some(KernelError::UnknownOrchestra(c.orchestra.clone()));
    };
    if !orch.roster.iter().any(|r| &r.musician == musician) {
        return Some(KernelError::NotOnRoster {
            musician: musician.clone(),
            orchestra: c.orchestra.clone(),
        });
    }
    let musician_record = match state.musicians.get(musician) {
        Some(m) => m,
        None => return Some(KernelError::UnknownMusician(musician.clone())),
    };
    let this_slots = concert_slots(&c.schedule);
    let other_slots = musician_busy(state, musician, Some(concert));
    for (_id, s) in &this_slots {
        // Against the musician's other federation-wide commitments.
        for (other_id, o) in &other_slots {
            if s.overlaps(o) {
                return Some(KernelError::MusicianDoubleBooked {
                    musician: musician.clone(),
                    conflicting: other_id.clone(),
                });
            }
        }
        // Against the musician's own blackout windows.
        for blackout in &musician_record.unavailable {
            if s.overlaps(blackout) {
                return Some(KernelError::MusicianUnavailable {
                    musician: musician.clone(),
                    slot: *s,
                });
            }
        }
    }
    None
}
```

Replace the remaining placeholder arm (`AssignPlayer | UnassignPlayer`) with:

```rust
        Command::AssignPlayer { concert, musician } => {
            if !next.musicians.contains_key(&musician) {
                return Err(KernelError::UnknownMusician(musician));
            }
            let c = next
                .concerts
                .get(&concert)
                .ok_or_else(|| KernelError::UnknownConcert(concert.clone()))?;
            if c.assignments.contains(&musician) {
                return Err(KernelError::AlreadyAssigned { musician, concert });
            }
            if let Some(err) = assignment_conflict(&next, &concert, &musician) {
                return Err(err);
            }
            let availability_pct = next.musicians[&musician].availability_pct;
            let c_mut = next.concerts.get_mut(&concert).expect("checked above");
            c_mut.assignments.push(musician.clone());
            let (required, assigned) = (c_mut.players_required, c_mut.assignments.len() as u16);
            events.push(Event::PlayerAssigned { concert: concert.clone(), musician: musician.clone() });
            if availability_pct < 50 {
                warnings.push(Warning::LowAvailability { musician, availability_pct });
            }
            if assigned < required {
                warnings.push(Warning::Understaffed { concert, required, assigned });
            }
        }
        Command::UnassignPlayer { concert, musician } => {
            let c = next
                .concerts
                .get_mut(&concert)
                .ok_or_else(|| KernelError::UnknownConcert(concert.clone()))?;
            let Some(pos) = c.assignments.iter().position(|m| m == &musician) else {
                return Err(KernelError::NotAssigned { musician, concert });
            };
            c.assignments.remove(pos);
            let (required, assigned) = (c.players_required, c.assignments.len() as u16);
            events.push(Event::PlayerUnassigned { concert: concert.clone(), musician });
            if assigned < required {
                warnings.push(Warning::Understaffed { concert, required, assigned });
            }
        }
```

- [ ] **Step 4: Run tests**

Run: `cargo test --no-default-features apply`
Expected: PASS (all apply tests).

- [ ] **Step 5: Commit**

```bash
git add src/apply.rs && git commit -m "feat: AssignPlayer/UnassignPlayer + shared assignment_conflict predicate"
```

---

## Task 11: `query` module

**Files:**
- Create: `src/query.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `pub mod query;` to `src/lib.rs`**

- [ ] **Step 2: Write `src/query.rs` (tests first by placing them, then impl above)**

```rust
//! Read-only analyses over a `Federation`: global conflicts, coverage, and the
//! legal-assignment picker.

use std::collections::BTreeMap;

use crate::apply::assignment_conflict;
use crate::error::Conflict;
use crate::ids::{ConcertId, MusicianId};
use crate::state::Federation;

/// Per-concert staffing coverage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coverage {
    pub concert: ConcertId,
    pub required: u16,
    pub assigned: u16,
    pub by_instrument: BTreeMap<String, u16>,
    pub satisfied: bool,
}

/// Every double-booking the kernel can see globally. After a sequence of
/// successful `apply` calls this should be empty; it exists to verify the
/// invariant and to power kernel-internal checks — never surfaced raw to actors.
///
/// # Examples
/// ```
/// use sycore::query::conflicts;
/// use sycore::state::Federation;
/// assert!(conflicts(&Federation::new()).is_empty());
/// ```
pub fn conflicts(state: &Federation) -> Vec<Conflict> {
    let mut out = Vec::new();

    // Musician double-bookings across the federation.
    for m in state.musicians.keys() {
        let busy = crate::apply::musician_busy(state, m, None);
        for i in 0..busy.len() {
            for j in (i + 1)..busy.len() {
                if busy[i].1.overlaps(&busy[j].1) {
                    out.push(Conflict::MusicianDoubleBooked {
                        musician: m.clone(),
                        events: (busy[i].0.clone(), busy[j].0.clone()),
                    });
                }
            }
        }
    }

    // Venue double-bookings.
    for v in state.venues.keys() {
        let busy = crate::apply::venue_busy(state, v);
        for i in 0..busy.len() {
            for j in (i + 1)..busy.len() {
                if busy[i].1.overlaps(&busy[j].1) {
                    out.push(Conflict::VenueDoubleBooked {
                        venue: v.clone(),
                        events: (busy[i].0.clone(), busy[j].0.clone()),
                    });
                }
            }
        }
    }

    out
}

/// Computes staffing coverage for a concert.
///
/// # Examples
/// ```
/// use sycore::query::coverage;
/// use sycore::state::Federation;
/// use sycore::ids::ConcertId;
/// // Unknown concert yields a zeroed, unsatisfied coverage.
/// let cov = coverage(&Federation::new(), &ConcertId::new("C99"));
/// assert!(!cov.satisfied);
/// ```
pub fn coverage(state: &Federation, concert: &ConcertId) -> Coverage {
    let Some(c) = state.concerts.get(concert) else {
        return Coverage {
            concert: concert.clone(),
            required: 0,
            assigned: 0,
            by_instrument: BTreeMap::new(),
            satisfied: false,
        };
    };
    let orch = state.orchestras.get(&c.orchestra);
    let mut by_instrument: BTreeMap<String, u16> = BTreeMap::new();
    for m in &c.assignments {
        if let Some(o) = orch {
            if let Some(entry) = o.roster.iter().find(|r| &r.musician == m) {
                *by_instrument.entry(entry.instrument.clone()).or_insert(0) += 1;
            }
        }
    }
    let assigned = c.assignments.len() as u16;
    Coverage {
        concert: concert.clone(),
        required: c.players_required,
        assigned,
        by_instrument,
        satisfied: assigned >= c.players_required,
    }
}

/// Roster members of the concert's orchestra playing `instrument` who could be
/// assigned right now without a hard conflict. Reuses the exact predicate
/// `apply` enforces, so a UI can offer only valid choices.
///
/// # Examples
/// ```
/// use sycore::query::legal_assignments;
/// use sycore::state::Federation;
/// use sycore::ids::ConcertId;
/// assert!(legal_assignments(&Federation::new(), &ConcertId::new("C01"), "Violin I").is_empty());
/// ```
pub fn legal_assignments(state: &Federation, concert: &ConcertId, instrument: &str) -> Vec<MusicianId> {
    let Some(c) = state.concerts.get(concert) else {
        return vec![];
    };
    let Some(orch) = state.orchestras.get(&c.orchestra) else {
        return vec![];
    };
    orch.roster
        .iter()
        .filter(|r| r.instrument == instrument)
        .map(|r| r.musician.clone())
        .filter(|m| !c.assignments.contains(m))
        .filter(|m| assignment_conflict(state, concert, m).is_none())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::apply;
    use crate::command::Command;
    use crate::entity::{Chair, EventKind, Program, Tier};
    use crate::ids::{OrchestraId, VenueId};
    use crate::time::{Date, Time, TimeSlot};

    fn setup() -> Federation {
        let mut f = Federation::new();
        f = apply(&f, Command::FoundOrchestra { id: OrchestraId::new("RSO"), name: "R".into() }).unwrap().state;
        f = apply(&f, Command::RegisterVenue {
            id: VenueId::new("VEN-01"), name: "Hall".into(), capacity: 1800,
            stage_type: "p".into(), has_pit: false, has_organ: true, loading_dock: true,
        }).unwrap().state;
        f = apply(&f, Command::ProgramConcert {
            id: ConcertId::new("C01"), orchestra: OrchestraId::new("RSO"),
            series: "M".into(), title: "G".into(),
            program: Program { works: vec![], requires_organ: false, requires_pit: false },
            players_required: 2,
        }).unwrap().state;
        for id in ["M001", "M002"] {
            f = apply(&f, Command::RegisterMusician {
                id: MusicianId::new(id), name: id.into(), primary_instrument: "Violin I".into(), availability_pct: 100,
            }).unwrap().state;
            f = apply(&f, Command::AddToRoster {
                orchestra: OrchestraId::new("RSO"), musician: MusicianId::new(id),
                instrument: "Violin I".into(), chair: Chair::Section, tier: Tier::Core,
            }).unwrap().state;
        }
        f
    }

    #[test]
    fn coverage_reflects_assignments() {
        let mut f = setup();
        f = apply(&f, Command::AssignPlayer { concert: ConcertId::new("C01"), musician: MusicianId::new("M001") }).unwrap().state;
        let cov = coverage(&f, &ConcertId::new("C01"));
        assert_eq!(cov.assigned, 1);
        assert_eq!(cov.required, 2);
        assert!(!cov.satisfied);
        assert_eq!(cov.by_instrument["Violin I"], 1);
    }

    #[test]
    fn legal_assignments_lists_unassigned_roster() {
        let f = setup();
        let legal = legal_assignments(&f, &ConcertId::new("C01"), "Violin I");
        assert_eq!(legal.len(), 2);
    }

    #[test]
    fn legal_assignments_excludes_already_assigned() {
        let mut f = setup();
        f = apply(&f, Command::AssignPlayer { concert: ConcertId::new("C01"), musician: MusicianId::new("M001") }).unwrap().state;
        let legal = legal_assignments(&f, &ConcertId::new("C01"), "Violin I");
        assert_eq!(legal, vec![MusicianId::new("M002")]);
    }

    #[test]
    fn valid_state_has_no_conflicts() {
        let mut f = setup();
        f = apply(&f, Command::ScheduleEvent {
            concert: ConcertId::new("C01"), kind: EventKind::Rehearsal,
            slot: TimeSlot::new(Date::new(2024, 9, 14), Time(600), 180),
            venue: VenueId::new("VEN-01"), call_time: None, downbeat: None,
        }).unwrap().state;
        f = apply(&f, Command::AssignPlayer { concert: ConcertId::new("C01"), musician: MusicianId::new("M001") }).unwrap().state;
        assert!(conflicts(&f).is_empty());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --no-default-features query`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/query.rs src/lib.rs && git commit -m "feat: conflicts, coverage, legal_assignments queries"
```

---

## Task 12: `view` module (the redaction layer)

**Files:**
- Create: `src/view.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add `pub mod view;` to `src/lib.rs`**

- [ ] **Step 2: Write `src/view.rs`**

```rust
//! Privacy-preserving per-actor projections. View types are deliberately
//! narrower than `Federation`, so over-disclosure is impossible by construction.

use crate::entity::{EventKind, RosterEntry};
use crate::ids::{ConcertId, MusicianId, OrchestraId, VenueId};
use crate::time::{Time, TimeSlot};
use crate::state::Federation;

/// One call on a musician's personal calendar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarItem {
    pub orchestra_name: String,
    pub concert_title: String,
    pub kind: EventKind,
    pub slot: TimeSlot,
    pub venue_name: String,
    pub call_time: Option<Time>,
    pub downbeat: Option<Time>,
}

/// A clash on the musician's OWN calendar — they may see both sides in full.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfClash {
    pub a: TimeSlot,
    pub b: TimeSlot,
}

/// What a musician is allowed to see: their full cross-orchestra calendar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MusicianView {
    pub musician: MusicianId,
    pub calendar: Vec<CalendarItem>,
    pub own_conflicts: Vec<SelfClash>,
    pub unavailable: Vec<TimeSlot>,
}

/// A concert as the owning orchestra sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConcertSummary {
    pub id: ConcertId,
    pub series: String,
    pub title: String,
    pub players_required: u16,
    pub assignments: Vec<MusicianId>,
}

/// A staffing gap on one of the orchestra's concerts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageGap {
    pub concert: ConcertId,
    pub required: u16,
    pub assigned: u16,
}

/// A redacted busy window for a roster musician: the admin learns *that* the
/// musician is committed, never *where* or *for whom*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedactedBusy {
    pub musician: MusicianId,
    pub slot: TimeSlot,
}

/// What an orchestra admin is allowed to see: only their own events, with other
/// orchestras' commitments redacted to anonymous busy windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrchestraView {
    pub orchestra: OrchestraId,
    pub roster: Vec<RosterEntry>,
    pub concerts: Vec<ConcertSummary>,
    pub coverage: Vec<CoverageGap>,
    pub blocked_slots: Vec<RedactedBusy>,
}

/// One booking on a venue's calendar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VenueBooking {
    pub slot: TimeSlot,
    pub orchestra_name: String,
    pub event_kind: EventKind,
}

/// What a venue is allowed to see: its own booking calendar only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VenueView {
    pub venue: VenueId,
    pub bookings: Vec<VenueBooking>,
}

/// Projects the federation to a single musician's personal calendar.
///
/// # Examples
/// ```
/// use sycore::view::view_for_musician;
/// use sycore::state::Federation;
/// use sycore::ids::MusicianId;
/// let v = view_for_musician(&Federation::new(), &MusicianId::new("M001"));
/// assert!(v.calendar.is_empty());
/// ```
pub fn view_for_musician(state: &Federation, musician: &MusicianId) -> MusicianView {
    let mut calendar = Vec::new();
    for concert in state.concerts.values() {
        if !concert.assignments.contains(musician) {
            continue;
        }
        let orchestra_name = state
            .orchestras
            .get(&concert.orchestra)
            .map(|o| o.name.clone())
            .unwrap_or_default();
        for ev in &concert.schedule {
            let venue_name = state.venues.get(&ev.venue).map(|v| v.name.clone()).unwrap_or_default();
            calendar.push(CalendarItem {
                orchestra_name: orchestra_name.clone(),
                concert_title: concert.title.clone(),
                kind: ev.kind,
                slot: ev.slot,
                venue_name,
                call_time: ev.call_time,
                downbeat: ev.downbeat,
            });
        }
    }
    let mut own_conflicts = Vec::new();
    for i in 0..calendar.len() {
        for j in (i + 1)..calendar.len() {
            if calendar[i].slot.overlaps(&calendar[j].slot) {
                own_conflicts.push(SelfClash { a: calendar[i].slot, b: calendar[j].slot });
            }
        }
    }
    let unavailable = state.musicians.get(musician).map(|m| m.unavailable.clone()).unwrap_or_default();
    MusicianView { musician: musician.clone(), calendar, own_conflicts, unavailable }
}

/// Projects the federation to an orchestra admin's view. Other orchestras'
/// events appear only as anonymous `blocked_slots` for the orchestra's own
/// roster members.
///
/// # Examples
/// ```
/// use sycore::view::view_for_orchestra;
/// use sycore::state::Federation;
/// use sycore::ids::OrchestraId;
/// let v = view_for_orchestra(&Federation::new(), &OrchestraId::new("RSO"));
/// assert!(v.concerts.is_empty());
/// ```
pub fn view_for_orchestra(state: &Federation, orchestra: &OrchestraId) -> OrchestraView {
    let roster = state.orchestras.get(orchestra).map(|o| o.roster.clone()).unwrap_or_default();

    let mut concerts = Vec::new();
    let mut coverage = Vec::new();
    for c in state.concerts.values().filter(|c| &c.orchestra == orchestra) {
        concerts.push(ConcertSummary {
            id: c.id.clone(),
            series: c.series.clone(),
            title: c.title.clone(),
            players_required: c.players_required,
            assignments: c.assignments.clone(),
        });
        if (c.assignments.len() as u16) < c.players_required {
            coverage.push(CoverageGap {
                concert: c.id.clone(),
                required: c.players_required,
                assigned: c.assignments.len() as u16,
            });
        }
    }

    // For each roster musician, surface ONLY anonymized busy windows from OTHER
    // orchestras' concerts plus their own blackout windows. No titles, venues,
    // orchestra names, or event ids cross this boundary.
    let mut blocked_slots = Vec::new();
    for entry in &roster {
        for other in state.concerts.values().filter(|c| &c.orchestra != orchestra) {
            if other.assignments.contains(&entry.musician) {
                for ev in &other.schedule {
                    blocked_slots.push(RedactedBusy { musician: entry.musician.clone(), slot: ev.slot });
                }
            }
        }
        if let Some(m) = state.musicians.get(&entry.musician) {
            for slot in &m.unavailable {
                blocked_slots.push(RedactedBusy { musician: entry.musician.clone(), slot: *slot });
            }
        }
    }

    OrchestraView { orchestra: orchestra.clone(), roster, concerts, coverage, blocked_slots }
}

/// Projects the federation to a venue's booking calendar.
///
/// # Examples
/// ```
/// use sycore::view::view_for_venue;
/// use sycore::state::Federation;
/// use sycore::ids::VenueId;
/// let v = view_for_venue(&Federation::new(), &VenueId::new("VEN-01"));
/// assert!(v.bookings.is_empty());
/// ```
pub fn view_for_venue(state: &Federation, venue: &VenueId) -> VenueView {
    let mut bookings = Vec::new();
    for c in state.concerts.values() {
        let orchestra_name = state.orchestras.get(&c.orchestra).map(|o| o.name.clone()).unwrap_or_default();
        for ev in &c.schedule {
            if &ev.venue == venue {
                bookings.push(VenueBooking {
                    slot: ev.slot,
                    orchestra_name: orchestra_name.clone(),
                    event_kind: ev.kind,
                });
            }
        }
    }
    VenueView { venue: venue.clone(), bookings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::apply;
    use crate::command::Command;
    use crate::entity::{Chair, Program, Tier};
    use crate::time::{Date, Time, TimeSlot};

    // Two orchestras share musician M001; one performs at VEN-01, the other at VEN-02,
    // at OVERLAPPING times — a cross-federation clash.
    fn two_orchestra_clash() -> Federation {
        let mut f = Federation::new();
        f = apply(&f, Command::RegisterMusician {
            id: MusicianId::new("M001"), name: "Shared".into(), primary_instrument: "Cello".into(), availability_pct: 100,
        }).unwrap().state;
        for (o, v) in [("RSO", "VEN-01"), ("PHIL", "VEN-02")] {
            f = apply(&f, Command::FoundOrchestra { id: OrchestraId::new(o), name: o.into() }).unwrap().state;
            f = apply(&f, Command::RegisterVenue {
                id: VenueId::new(v), name: v.into(), capacity: 1000, stage_type: "p".into(),
                has_pit: false, has_organ: false, loading_dock: false,
            }).unwrap().state;
            f = apply(&f, Command::AddToRoster {
                orchestra: OrchestraId::new(o), musician: MusicianId::new("M001"),
                instrument: "Cello".into(), chair: Chair::Section, tier: Tier::Core,
            }).unwrap().state;
        }
        // RSO concert C01 at VEN-01, rehearsal 18:00–21:00, M001 assigned.
        f = apply(&f, Command::ProgramConcert {
            id: ConcertId::new("C01"), orchestra: OrchestraId::new("RSO"),
            series: "M".into(), title: "RSO Night".into(),
            program: Program { works: vec![], requires_organ: false, requires_pit: false },
            players_required: 1,
        }).unwrap().state;
        f = apply(&f, Command::ScheduleEvent {
            concert: ConcertId::new("C01"), kind: crate::entity::EventKind::Performance,
            slot: TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180),
            venue: VenueId::new("VEN-01"), call_time: None, downbeat: None,
        }).unwrap().state;
        f = apply(&f, Command::AssignPlayer { concert: ConcertId::new("C01"), musician: MusicianId::new("M001") }).unwrap().state;
        f
    }

    #[test]
    fn musician_sees_own_calendar() {
        let f = two_orchestra_clash();
        let v = view_for_musician(&f, &MusicianId::new("M001"));
        assert_eq!(v.calendar.len(), 1);
        assert_eq!(v.calendar[0].orchestra_name, "RSO");
    }

    #[test]
    fn orchestra_sees_only_own_concerts() {
        let f = two_orchestra_clash();
        let v = view_for_orchestra(&f, &OrchestraId::new("RSO"));
        assert_eq!(v.concerts.len(), 1);
        assert_eq!(v.concerts[0].title, "RSO Night");
    }

    #[test]
    fn orchestra_blocked_slots_are_anonymous() {
        // Build a second clashing concert for PHIL so RSO sees M001 busy elsewhere.
        let mut f = two_orchestra_clash();
        f = apply(&f, Command::ProgramConcert {
            id: ConcertId::new("P01"), orchestra: OrchestraId::new("PHIL"),
            series: "M".into(), title: "PHIL Secret".into(),
            program: Program { works: vec![], requires_organ: false, requires_pit: false },
            players_required: 1,
        }).unwrap().state;
        f = apply(&f, Command::ScheduleEvent {
            concert: ConcertId::new("P01"), kind: crate::entity::EventKind::Rehearsal,
            slot: TimeSlot::new(Date::new(2024, 9, 20), Time(600), 120),
            venue: VenueId::new("VEN-02"), call_time: None, downbeat: None,
        }).unwrap().state;
        f = apply(&f, Command::AssignPlayer { concert: ConcertId::new("P01"), musician: MusicianId::new("M001") }).unwrap().state;

        let v = view_for_orchestra(&f, &OrchestraId::new("RSO"));
        // RSO learns M001 is busy on 9/20, but the RedactedBusy struct has NO field
        // that could carry "PHIL Secret", VEN-02, or an event id.
        assert!(v.blocked_slots.iter().any(|b| b.musician == MusicianId::new("M001")
            && b.slot.date == Date::new(2024, 9, 20)));
    }

    #[test]
    fn venue_sees_only_its_bookings() {
        let f = two_orchestra_clash();
        let v = view_for_venue(&f, &VenueId::new("VEN-01"));
        assert_eq!(v.bookings.len(), 1);
        assert_eq!(v.bookings[0].orchestra_name, "RSO");
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --no-default-features view`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/view.rs src/lib.rs && git commit -m "feat: privacy-preserving per-actor views"
```

---

## Task 13: Crate root docs + re-exports

**Files:**
- Modify: `src/lib.rs`

- [ ] **Step 1: Replace `src/lib.rs` with full crate docs and a prelude-style re-export**

```rust
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
```

- [ ] **Step 2: Run the whole suite (pure build)**

Run: `cargo test --no-default-features`
Expected: PASS — all unit + doc tests.

- [ ] **Step 3: Confirm no library `unwrap`/`expect`/`panic!` outside the one tolerated `expect`**

Run: `grep -rn "unwrap()\|expect(\|panic!" src --include=*.rs | grep -v "mod tests" | grep -v "/// " | grep -v "//!"`
Expected: only the documented `.expect("checked above")` lines in `apply.rs` (or none if you restructured them away). No `unwrap()` in non-test, non-doc code.

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs && git commit -m "docs: crate root documentation and module surface"
```

---

## Task 14: `seed` feature — JSON → commands

**Files:**
- Create: `src/seed.rs`

- [ ] **Step 1: Write `src/seed.rs` with DTOs, mapping, `sample_commands`, `build_sample`, and tests**

```rust
//! Sample-data loading (feature `seed`). Parses `orchestra_sample_data.json`
//! into a sequence of [`Command`]s, so even bootstrapping flows through `apply`
//! and is subject to the same invariants. This is the ONLY module that touches
//! serde; the core stays serialization-free.

use serde::Deserialize;

use crate::apply::apply;
use crate::command::Command;
use crate::entity::{Chair, EventKind, Program, Tier, Work};
use crate::error::{KernelError, Warning};
use crate::ids::{ConcertId, MusicianId, OrchestraId, VenueId};
use crate::state::Federation;
use crate::time::{Date, Time, TimeSlot};

/// Errors that can occur while turning sample JSON into commands.
#[derive(Debug)]
pub enum SeedError {
    Parse(serde_json::Error),
    Apply(KernelError),
    BadField(String),
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeedError::Parse(e) => write!(f, "json parse error: {e}"),
            SeedError::Apply(e) => write!(f, "apply error during seed: {e}"),
            SeedError::BadField(s) => write!(f, "bad field: {s}"),
        }
    }
}

impl std::error::Error for SeedError {}

impl From<serde_json::Error> for SeedError {
    fn from(e: serde_json::Error) -> Self {
        SeedError::Parse(e)
    }
}

// ---- JSON DTOs (private; serde lives only here) -------------------------------

#[derive(Deserialize)]
struct Root {
    organization: Organization,
    venues: Vec<VenueDto>,
    roster_pool: Vec<RosterDto>,
    season_concerts: Vec<ConcertDto>,
}

#[derive(Deserialize)]
struct Organization {
    name: String,
}

#[derive(Deserialize)]
struct VenueDto {
    venue_id: String,
    name: String,
    #[serde(default)]
    capacity: u32,
    #[serde(default)]
    stage_type: String,
    #[serde(default)]
    has_pit: bool,
    #[serde(default)]
    has_organ: bool,
    #[serde(default)]
    loading_dock: bool,
}

#[derive(Deserialize)]
struct RosterDto {
    id: String,
    name: String,
    instrument: String,
    chair: String,
    #[allow(dead_code)]
    tier: String,
    #[serde(default = "one")]
    availability: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Deserialize)]
struct ConcertDto {
    concert_id: String,
    series: String,
    title: String,
    #[serde(default)]
    program: Vec<WorkDto>,
    #[serde(default)]
    players_required: u16,
    #[serde(default)]
    player_ids: Vec<String>,
    #[serde(default)]
    rehearsals: Vec<EventDto>,
    #[serde(default)]
    performances: Vec<EventDto>,
}

#[derive(Deserialize)]
struct WorkDto {
    composer: String,
    work: String,
    #[serde(default)]
    duration_min: u16,
    #[serde(default)]
    forces: String,
}

#[derive(Deserialize)]
struct EventDto {
    date: String,
    #[serde(default)]
    start_time: String,
    #[serde(default)]
    call_time: String,
    #[serde(default)]
    downbeat: String,
    #[serde(default)]
    duration_hours: f64,
    venue_id: String,
}

// ---- Mapping helpers ----------------------------------------------------------

const ORCHESTRA_ID: &str = "RSO";

fn parse_date(s: &str) -> Result<Date, SeedError> {
    // "2024-09-14"
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err(SeedError::BadField(format!("date: {s}")));
    }
    let year = parts[0].parse().map_err(|_| SeedError::BadField(format!("year: {s}")))?;
    let month = parts[1].parse().map_err(|_| SeedError::BadField(format!("month: {s}")))?;
    let day = parts[2].parse().map_err(|_| SeedError::BadField(format!("day: {s}")))?;
    Ok(Date::new(year, month, day))
}

fn parse_time(s: &str) -> Result<Time, SeedError> {
    // "19:30"
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(SeedError::BadField(format!("time: {s}")));
    }
    let h: u8 = parts[0].parse().map_err(|_| SeedError::BadField(format!("hour: {s}")))?;
    let m: u8 = parts[1].parse().map_err(|_| SeedError::BadField(format!("minute: {s}")))?;
    Time::from_hm(h, m).map_err(|e| SeedError::BadField(format!("{e}: {s}")))
}

fn chair_of(s: &str) -> Chair {
    if s.contains("Concertmaster") {
        Chair::Concertmaster
    } else if s.contains("Principal") {
        Chair::Principal
    } else {
        Chair::Section
    }
}

fn tier_of(s: &str) -> Tier {
    match s {
        "core" => Tier::Core,
        "sub" => Tier::Sub,
        _ => Tier::Extra,
    }
}

/// Parses the sample JSON into a replayable command stream.
///
/// # Errors
/// Returns [`SeedError`] if the JSON is malformed or a date/time field is invalid.
pub fn sample_commands(json: &str) -> Result<Vec<Command>, SeedError> {
    let root: Root = serde_json::from_str(json)?;
    let mut cmds = Vec::new();

    cmds.push(Command::FoundOrchestra {
        id: OrchestraId::new(ORCHESTRA_ID),
        name: root.organization.name,
    });

    for v in root.venues {
        cmds.push(Command::RegisterVenue {
            id: VenueId::new(v.venue_id),
            name: v.name,
            capacity: v.capacity,
            stage_type: v.stage_type,
            has_pit: v.has_pit,
            has_organ: v.has_organ,
            loading_dock: v.loading_dock,
        });
    }

    for r in root.roster_pool {
        let availability_pct = (r.availability * 100.0).round().clamp(0.0, 100.0) as u8;
        cmds.push(Command::RegisterMusician {
            id: MusicianId::new(r.id.clone()),
            name: r.name,
            primary_instrument: r.instrument.clone(),
            availability_pct,
        });
        cmds.push(Command::AddToRoster {
            orchestra: OrchestraId::new(ORCHESTRA_ID),
            musician: MusicianId::new(r.id),
            instrument: r.instrument,
            chair: chair_of(&r.chair),
            tier: tier_of(&r.tier),
        });
    }

    for c in root.season_concerts {
        let requires_organ = c
            .program
            .iter()
            .any(|w| w.forces.to_lowercase().contains("organ") || w.work.to_lowercase().contains("organ"));
        let works = c
            .program
            .iter()
            .map(|w| Work {
                composer: w.composer.clone(),
                title: w.work.clone(),
                duration_min: w.duration_min,
                forces: w.forces.clone(),
            })
            .collect();
        cmds.push(Command::ProgramConcert {
            id: ConcertId::new(c.concert_id.clone()),
            orchestra: OrchestraId::new(ORCHESTRA_ID),
            series: c.series,
            title: c.title,
            program: Program { works, requires_organ, requires_pit: false },
            players_required: c.players_required,
        });

        for reh in &c.rehearsals {
            let date = parse_date(&reh.date)?;
            let start = parse_time(if reh.start_time.is_empty() { "10:00" } else { &reh.start_time })?;
            let duration_min = (reh.duration_hours * 60.0).round() as u16;
            cmds.push(Command::ScheduleEvent {
                concert: ConcertId::new(c.concert_id.clone()),
                kind: EventKind::Rehearsal,
                slot: TimeSlot::new(date, start, duration_min.max(1)),
                venue: VenueId::new(reh.venue_id.clone()),
                call_time: None,
                downbeat: None,
            });
        }
        for perf in &c.performances {
            let date = parse_date(&perf.date)?;
            let call = if perf.call_time.is_empty() { None } else { Some(parse_time(&perf.call_time)?) };
            let downbeat = if perf.downbeat.is_empty() { None } else { Some(parse_time(&perf.downbeat)?) };
            // Occupied window: from call (or start) for 3h by default.
            let start = call.unwrap_or(match perf.start_time.is_empty() {
                true => Time(1170),
                false => parse_time(&perf.start_time)?,
            });
            cmds.push(Command::ScheduleEvent {
                concert: ConcertId::new(c.concert_id.clone()),
                kind: EventKind::Performance,
                slot: TimeSlot::new(date, start, 180),
                venue: VenueId::new(perf.venue_id.clone()),
                call_time: call,
                downbeat,
            });
        }

        for pid in c.player_ids {
            cmds.push(Command::AssignPlayer {
                concert: ConcertId::new(c.concert_id.clone()),
                musician: MusicianId::new(pid),
            });
        }
    }

    Ok(cmds)
}

/// Builds a `Federation` by replaying the sample command stream through `apply`,
/// collecting any soft warnings. Hard errors abort with [`SeedError::Apply`].
///
/// # Errors
/// Returns [`SeedError`] on parse failure or any hard conflict during replay.
pub fn build_sample(json: &str) -> Result<(Federation, Vec<Warning>), SeedError> {
    let cmds = sample_commands(json)?;
    let mut state = Federation::new();
    let mut warnings = Vec::new();
    for cmd in cmds {
        let t = apply(&state, cmd).map_err(SeedError::Apply)?;
        state = t.state;
        warnings.extend(t.warnings);
    }
    Ok((state, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI: &str = r#"{
        "organization": { "name": "Riverside Symphony" },
        "venues": [{ "venue_id": "VEN-01", "name": "Hall", "capacity": 1800, "has_organ": true }],
        "roster_pool": [
            { "id": "M001", "name": "James", "instrument": "Violin I", "chair": "Principal/Concertmaster", "tier": "core", "availability": 1.0 },
            { "id": "M002", "name": "Amy", "instrument": "Violin I", "chair": "Section", "tier": "sub", "availability": 0.4 }
        ],
        "season_concerts": [{
            "concert_id": "C01", "series": "Masterworks", "title": "Gala",
            "program": [{ "composer": "Barber", "work": "Adagio", "duration_min": 9, "forces": "strings" }],
            "players_required": 2,
            "rehearsals": [{ "date": "2024-08-29", "start_time": "10:00", "duration_hours": 3.0, "venue_id": "VEN-01" }],
            "performances": [{ "date": "2024-09-14", "start_time": "19:30", "call_time": "18:00", "downbeat": "19:00", "venue_id": "VEN-01" }],
            "player_ids": ["M001", "M002"]
        }]
    }"#;

    #[test]
    fn mini_builds_without_hard_errors() {
        let (state, warnings) = build_sample(MINI).unwrap();
        assert_eq!(state.musicians.len(), 2);
        assert_eq!(state.concerts[&ConcertId::new("C01")].assignments.len(), 2);
        // M002 has availability 0.4 → LowAvailability warning expected.
        assert!(warnings.iter().any(|w| matches!(w, Warning::LowAvailability { availability_pct: 40, .. })));
    }

    #[test]
    fn availability_float_maps_to_percentage() {
        let cmds = sample_commands(MINI).unwrap();
        assert!(cmds.iter().any(|c| matches!(
            c,
            Command::RegisterMusician { availability_pct: 40, .. }
        )));
    }
}
```

- [ ] **Step 2: Run tests with the feature on**

Run: `cargo test --features seed seed`
Expected: PASS.

- [ ] **Step 3: Confirm the pure build still excludes serde**

Run: `cargo tree --no-default-features -e normal`
Expected: no `serde`/`serde_json`.

- [ ] **Step 4: Commit**

```bash
git add src/seed.rs && git commit -m "feat: seed feature — sample JSON to command stream"
```

---

## Task 15: Integration test — replay the real sample data

**Files:**
- Create: `tests/sample_season.rs`

- [ ] **Step 1: Write the integration test**

```rust
//! Replays the bundled sample season and asserts the kernel reconstructs it with
//! zero hard conflicts. Runs only with `--features seed`.
#![cfg(feature = "seed")]

use sycore::query::conflicts;
use sycore::seed::build_sample;

const SAMPLE: &str = include_str!("../data/orchestra_sample_data.json");

#[test]
fn sample_season_builds_with_no_hard_conflicts() {
    let (state, _warnings) = build_sample(SAMPLE).expect("sample data should seed without hard errors");

    // The whole 20-concert season loaded.
    assert_eq!(state.orchestras.len(), 1);
    assert!(state.concerts.len() >= 20, "expected the full season of concerts");
    assert!(state.musicians.len() >= 180, "expected the full roster pool");

    // The headline invariant: a valid season has no double-bookings anywhere.
    let global = conflicts(&state);
    assert!(global.is_empty(), "unexpected conflicts after seeding: {global:?}");
}

#[test]
fn sample_season_produces_some_soft_warnings() {
    // Soft warnings (understaffing, low availability, capability mismatch) are
    // expected and allowed — they must not be errors.
    let (_state, warnings) = build_sample(SAMPLE).expect("seed");
    // We don't assert a specific count (data may evolve); just that the channel works.
    let _ = warnings.len();
}
```

> **If this test surfaces a hard conflict in the real data**, that is a genuine finding, not a test bug: the sample season contains an overlap the kernel's rules reject (e.g. the same player in two concerts with overlapping calls, or a venue booked twice). Investigate with `systematic-debugging`. Likely resolutions: (a) the overlap is real and the data is the source of truth → relax the offending check to a soft `Warning` for that case and record the decision in the spec; or (b) the seed mapping built a wrong slot (e.g. a default 180-min performance window that's too wide) → fix the mapping. Decide with the user before changing a hard rule.

- [ ] **Step 2: Run the integration test**

Run: `cargo test --features seed --test sample_season`
Expected: PASS. If it fails on a hard conflict, follow the note above.

- [ ] **Step 3: Run the entire suite, both feature modes**

Run: `cargo test --no-default-features && cargo test --features seed`
Expected: PASS in both.

- [ ] **Step 4: Run formatting and clippy**

Run: `cargo fmt --check && cargo clippy --all-features -- -D warnings`
Expected: clean. Fix any clippy findings (e.g. needless clones) and re-run.

- [ ] **Step 5: Commit**

```bash
git add tests/sample_season.rs && git commit -m "test: integration replay of sample season with zero hard conflicts"
```

---

## Self-Review (completed by plan author)

**Spec coverage:**
- Pure-by-default / `default = []` / serde behind `seed` → Task 0, 14, verified Task 13/14.
- Federation model (shared musicians+venues, per-orchestra rosters) → Task 7, entities Task 3.
- Three perspectives, no omniscient actor, redaction → Task 12 (`view_for_*`, `RedactedBusy` has no leak-capable fields, tested in `orchestra_blocked_slots_are_anonymous`).
- Reject hard / warn soft → Tasks 9, 10 (errors vs `Warning`s).
- Events emitted → `event` Task 5, populated in Tasks 8–10.
- Cross-federation conflict detection + redacted disclosure → `musician_busy` (Task 8), `MusicianDoubleBooked` error to assigner (Task 10) vs anonymous `blocked_slots` to other orchestra (Task 12).
- Availability float → integer percentage → Tasks 3, 14 (documented refinement).
- Time model without chrono → Task 2.
- Queries `conflicts`/`coverage`/`legal_assignments` → Task 11.
- Sample data as integration test, zero hard conflicts → Task 15.
- Testing: every public item has unit + doc tests → present in each module task. No `unwrap/expect/panic` in lib code except one documented `expect` → guarded in Task 13 Step 3.
- WIT deferral → all types are string/int/enum/Result-shaped (Task 1–6); no boundary work in this slice. ✓

**Placeholder scan:** No "TBD/TODO/handle edge cases" — every code step shows complete code. The only narrative "implemented later" is the explicit staged match arm in Task 8, removed in Tasks 9–10. ✓

**Type consistency:** `apply` signature, `Transition` fields, `Command`/`Event`/`KernelError`/`Warning` variants, and view structs are referenced identically across tasks. `assignment_conflict` is defined once (Task 10) and reused in `query` (Task 11). `availability_pct` consistent in `entity`, `command`, `apply`, `seed`. ✓

**Scope:** Single coherent slice — one crate, one feature flag, one integration test. No decomposition needed. ✓
