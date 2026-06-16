# SyCore Domain Kernel — Slice 1 Design

**Date:** 2026-06-16
**Status:** Approved for planning
**Slice:** 1 of N — Foundational `SeasonState`/`Federation` model + Scheduling & Assignment conflict logic

## Purpose

`sycore` is the pure, delivery-agnostic domain kernel for managing musicians who
play for various orchestras at shared venues. It is the central library that
future applications — a musician-facing app, an orchestra-admin app, a
venue-facing app — all drive, unchanged. The kernel holds the complete truth of
the federation and answers, for any proposed change: *is this legal, what
happened, and what should each actor be allowed to see?*

The sample data in `data/orchestra_sample_data.json` (Riverside Symphony
Orchestra, 2024–2025 season: 180-musician roster pool, 5 venues, 20 concerts
with programs, rehearsals, and performances) is the foundation of the data model
and the headline integration test.

## Scope

"Full season management" decomposes into five bounded contexts that share data:

| Context | Holds | Decision-heavy? |
|---|---|---|
| Roster | musicians, instruments, tiers, availability | Light |
| Venues | venue specs, capabilities | Light |
| Repertoire | concerts, programs, works, publishers, parts | Light |
| Scheduling | rehearsals & performances on dates/times/venues | **Heavy** |
| Assignment | which musicians play which concert | **Heavy** |

The kernel's value concentrates in **Scheduling + Assignment**: the pure
functions that answer "is this legal?" and "what conflicts exist?". The other
three are immutable inputs those functions read.

**Slice 1** delivers the foundational `Federation` data model (all five contexts
as pure, validated types) **plus** the Scheduling + Assignment transition and
conflict-detection logic — because the data model cannot be meaningfully tested
without at least one decision engine exercising it.

### Explicitly out of scope for Slice 1 (each a candidate later slice)

- Rich repertoire: publishers, full `parts_list`, rental tracking
- The WIT world + wasm component packaging + JS/Python hosts
- Persistence / event store (events are produced but not stored by the kernel)
- Notifications, auth, any application UI
- Multi-season support; season-summary statistics as a derived query

## Core Design Decisions (confirmed with user)

1. **Pure by default.** No filesystem, network, clock, randomness, or
   environment access in the core. `default = []`. Serde/JSON and sample-data
   seeding live behind an opt-in `seed` feature so the core never depends on a
   format crate.
2. **Federation model.** Musicians and venues are **global, shared** entities.
   Orchestras each have their own roster (drawn from the shared pool), programs,
   schedule, and assignments. The kernel holds the complete truth.
3. **No omniscient actor.** Three perspectives — musician, orchestra admin,
   venue — each see only what they may access. All application reads go through
   privacy-preserving `view_for_*` projections.
4. **Conflict policy: reject hard, warn on soft.** Physically-impossible
   conflicts fail the command (`Err`); soft conflicts (under-staffing, low
   availability, capability mismatch) succeed and return a `Warning`.
5. **Events emitted.** Each successful transition returns the new state **and** a
   list of typed domain events describing what changed.
6. **Cross-federation conflict detection with redacted disclosure.** A musician
   cannot be at two orchestras' overlapping calls; a venue cannot be booked
   twice. The kernel detects this globally, but the explanation surfaced to each
   actor is redacted to what they're allowed to know.
7. **Pure Rust now, WIT later.** Public API designed to be WIT-mappable today;
   actual component boundary deferred to a later slice.

## Architecture

One immutable state value (`Federation`), total transition functions
`(state, command) -> Result<Transition, KernelError>`, and read-only
projections. The kernel holds the whole federation; no actor is omniscient.

```
                 ┌─────────────────────────────────────┐
   Command  ───▶ │  apply(&Federation, Command)         │ ──▶ Result<Transition, KernelError>
                 │    → Transition { state, events,     │       (hard conflict = Err)
                 │                   warnings }          │
                 └─────────────────────────────────────┘
   Federation (immutable full truth)
        │
        ├──▶ view_for_musician(id)   → MusicianView   (own calendar, all orchestras)
        ├──▶ view_for_orchestra(id)  → OrchestraView  (own events only; others redacted)
        ├──▶ view_for_venue(id)      → VenueView      (own bookings only)
        │
        └──▶ queries: conflicts(), coverage(concert), legal_assignments(concert, instrument)
```

### Module layout

Each file has one clear purpose (per project CLAUDE.md).

| Module | Responsibility |
|---|---|
| `ids` | Type-safe newtype IDs (`MusicianId`, `OrchestraId`, `VenueId`, `ConcertId`, `EventId`) |
| `time` | `Date`, `Time` (minutes-from-midnight), `TimeSlot` + interval overlap — no `chrono`, integer-backed, WIT-mappable |
| `entity` | `Musician`, `Orchestra`, `Venue` (+capabilities), `Section`/`Instrument`, `Chair`, `Tier`, `Concert`, `Program`, `CalendarEvent` |
| `state` | `Federation` aggregate + constructors |
| `command` | `Command` enum |
| `event` | `Event` enum |
| `error` | `KernelError`, `Warning`, `Conflict` |
| `apply` | the transition function |
| `query` | `conflicts`, `coverage`, `legal_assignments` |
| `view` | `view_for_*` + the view types |
| `seed` *(feature)* | parse `orchestra_sample_data.json` → a sequence of `Command`s |

The seed module produces **Commands**, not a pre-built state, so even loading the
sample data goes through `apply` and is subject to the same invariants. The
sample data thereby doubles as a built-in integration test.

## Core Types

### Identifiers

Newtypes wrapping the string IDs from the data (`"M001"`, `"VEN-01"`, `"C01"`),
so the compiler prevents passing a `VenueId` where a `ConcertId` is expected.

### Time (pure, integer-backed)

```
Date { year: u16, month: u8, day: u8 }
Time(u16)                      // minutes since midnight; 19:30 → 1170
TimeSlot { date: Date, start: Time, duration_min: u16 }
    fn overlaps(&self, other: &TimeSlot) -> bool   // same date && intervals intersect
```

A performance's occupied window runs from `call_time` to end-of-performance; a
rehearsal's from `start_time` for `duration_hours`. Overlap on these windows
defines a double-booking. No `chrono` dependency — integer-backed and trivially
WIT-mappable.

### Entities (immutable records)

- `Musician { id, name, primary_instrument, unavailable: Vec<TimeSlot> }` —
  *global*. `unavailable` is the musician's own blackout windows.
- `Orchestra { id, name, roster: Vec<RosterEntry> }` where
  `RosterEntry { musician: MusicianId, instrument, chair: Chair, tier: Tier }`.
  `Tier = Core | Sub | Extra`; `Chair = Principal | Section | Concertmaster | …`.
- `Venue { id, name, capacity, stage_type, has_pit, has_organ, loading_dock }` —
  *global, shared*.
- `Concert { id, orchestra, series, title, program: Program, players_required, assignments: Vec<MusicianId>, schedule: Vec<CalendarEvent> }`.
- `Program { works: Vec<Work> }`, `Work { composer, title, duration_min, forces, … }`
  — repertoire detail kept as data; publisher/parts modeled minimally in Slice 1.
- `CalendarEvent { id, kind: Rehearsal | Performance, slot: TimeSlot, venue: VenueId, call_time: Option<Time>, downbeat: Option<Time> }`.

### Federation (the whole truth)

```
Federation {
    musicians: Map<MusicianId, Musician>,    // shared pool
    venues:    Map<VenueId, Venue>,          // shared resources
    orchestras: Map<OrchestraId, Orchestra>, // each with roster + concerts
    concerts:  Map<ConcertId, Concert>,
}
```

### Availability modeling note

The sample's `availability: 0.0–1.0` is a *statistical* hint, not a schedulable
fact — a kernel cannot reason about "0.6 available" when checking a specific 7pm
call. In the kernel it becomes concrete `unavailable: Vec<TimeSlot>` windows. The
original float survives only in the seed layer as a soft-warning input (assigning
a low-availability musician → warning). Fuzzy source data is converted into
decidable state at the boundary, keeping the core total and deterministic.

## Commands

The only way to change state — enough to reconstruct the sample season:

```
RegisterMusician { id, name, primary_instrument }
RegisterVenue { id, name, capacity, stage_type, has_pit, has_organ, loading_dock }
FoundOrchestra { id, name }
AddToRoster { orchestra, musician, instrument, chair, tier }
SetUnavailable { musician, slots }
ProgramConcert { id, orchestra, series, title, program, players_required }
ScheduleEvent { concert, kind, slot, venue, call_time, downbeat }   // rehearsal or performance
AssignPlayer { concert, musician }
UnassignPlayer { concert, musician }
```

## Events

Mirror successful commands: `MusicianRegistered`, `VenueRegistered`,
`OrchestraFounded`, `MusicianAddedToRoster`, `UnavailabilitySet`,
`ConcertProgrammed`, `EventScheduled`, `PlayerAssigned`, `PlayerUnassigned`.

`apply` returns `Transition { state, events, warnings }`.

## Conflict Semantics

### Hard conflicts → `Err(KernelError)`, state unchanged

- Reference to a nonexistent musician / venue / orchestra / concert
- `AssignPlayer` for a musician not on that orchestra's roster
- `AssignPlayer` colliding with **any** of that musician's other calls (across
  all orchestras) on overlapping `TimeSlot`s — *the cross-federation check*
- `ScheduleEvent` at a venue already booked for an overlapping slot
- `AssignPlayer` onto a slot the musician marked `unavailable`
- Malformed time (`call_time` after `downbeat`; zero-duration slot)

### Soft conflicts → `Ok` + `Warning`

- Section/instrument coverage below `players_required` (under-staffed)
- Assigning a low-availability musician (the surviving float signal)
- Program needs a capability the venue lacks (e.g. `forces` implies organ,
  `has_organ = false`)
- Performance with no rehearsal scheduled

### Error type

```
KernelError = UnknownMusician(id) | UnknownVenue(id) | UnknownOrchestra(id)
            | UnknownConcert(id) | NotOnRoster{musician,orchestra}
            | MusicianDoubleBooked{musician, conflicting: EventId}
            | VenueDoubleBooked{venue, conflicting: EventId}
            | MusicianUnavailable{musician, slot} | InvalidTime{…} | DuplicateId(…)
```

`MusicianDoubleBooked` carries the conflicting `EventId`, but it is only ever
returned to the caller *making* the assignment. When the *other* orchestra later
renders its `OrchestraView`, the same underlying clash is translated into a
redacted `MusicianUnavailable`-style marker with no `EventId` and no
other-orchestra identity. Same fact in the kernel; two different disclosures
depending on who's asking.

## Views (the redaction layer)

Pure read-only projections. The view types are deliberately narrower than the
`Federation` — they can only express what their actor may know, so
over-disclosure is a compile-time impossibility, not a runtime check.

### `view_for_musician(state, id) -> MusicianView`

The musician sees their own world fully:

```
MusicianView {
    musician: MusicianId,
    calendar: Vec<CalendarItem>,   // every call from every orchestra they're assigned to
    own_conflicts: Vec<SelfClash>, // their own double-bookings, both sides named — their info
    unavailable: Vec<TimeSlot>,
}
CalendarItem { orchestra_name, concert_title, kind, slot, venue_name, call_time, downbeat }
```

### `view_for_orchestra(state, id) -> OrchestraView`

Only this orchestra's events; other orchestras redacted:

```
OrchestraView {
    orchestra: OrchestraId,
    roster: Vec<RosterEntry>,
    concerts: Vec<ConcertSummary>,      // programs, schedule, assignments for THIS orchestra
    coverage: Vec<CoverageGap>,         // soft warnings: section under target
    blocked_slots: Vec<RedactedBusy>,   // "M001 unavailable 2024-09-14 18:00–22:00" —
                                        //  no reason, no competing orchestra, no EventId
}
```

### `view_for_venue(state, id) -> VenueView`

Booking calendar only:

```
VenueView {
    venue: VenueId,
    bookings: Vec<VenueBooking>,   // { slot, orchestra_name, event_kind } — NO program, NO roster
}
```

## Queries

Used by applications and by `apply` internally for conflict checks:

- `conflicts(&Federation) -> Vec<Conflict>` — global computation (the kernel's
  own omniscient check; never surfaced raw to any actor)
- `coverage(&Federation, ConcertId) -> Coverage` — per-instrument required vs assigned
- `legal_assignments(&Federation, ConcertId, instrument) -> Vec<MusicianId>` —
  roster members assignable *without* a hard conflict (powers an app's "who can I
  add?" picker)

`legal_assignments` reuses the exact hard-conflict predicate that `apply`
enforces, so a UI picker can never offer a choice that `apply` would then reject.
One source of truth for "is this legal," consumed two ways.

## Testing

Per project CLAUDE.md — non-negotiable:

- Every public fn/type gets ≥1 unit test (happy path + edge/error) and ≥1 doc
  test demonstrating usage.
- Property-style tests for the hard invariants: after any successful `apply`, no
  musician and no venue is double-booked across the whole federation.
- **The sample data is the headline integration test**: replaying the full
  `seed`-generated command stream must reconstruct the season and produce **zero
  hard errors** (soft warnings allowed and snapshotted). Run under
  `--features seed`.
- No `unwrap`/`expect`/`panic!` in library code; `KernelError`/`Warning` carry
  everything.

## Boundary Deferral

Pure Rust now. The public API is designed WIT-mappable today (newtype IDs over
strings, integer time, `Result`, enums-with-payload → future WIT `variant`) so
the later WIT/component slice is a translation, not a redesign. No serde in the
core; the `seed` feature isolates JSON parsing.

## Success Criteria for Slice 1

- The kernel compiles pure (`cargo build --no-default-features`).
- The sample season seeds with zero hard conflicts.
- Every public item has unit + doc tests; `cargo test` and `cargo test --doc` pass.
- The three `view_for_*` projections demonstrably cannot leak cross-orchestra
  detail (enforced by their types + tested).
