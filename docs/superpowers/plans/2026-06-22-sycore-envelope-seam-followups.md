# SyCore Envelope-Seam Follow-ups — Implementation Plan & Roadmap

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement **Phase 1** task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Phase 2 is a sequenced roadmap, not executable tasks** — its items are blocked on substrate that does not exist yet; do not attempt to implement them.

**Goal:** Fortify the kernel guarantees the future FolkEngine envelope layer depends on (determinism, stable cross-seam type shape) as CI regression tests now, and record the substrate-blocked follow-ups as a sequenced roadmap.

**Architecture:** SyCore is the pure scheduling kernel beneath the FolkEngine substrate. The substrate will one day sign, CAS-address (BLAKE3), and selectively-disclose (Gordian Envelope / SHA-256 / dCBOR) the kernel's outputs. Per `docs/SPEC-sycore-envelope-boundary.md`, the kernel's only standing obligations are to stay pure/deterministic (obligation 1), keep cross-seam types canonically encodable without ambiguity (obligation 2), and not regress either. Phase 1 converts those two obligations from prose into tests. Everything else in that spec's Part 4 is blocked on substrate that isn't built yet — Phase 2 sequences it.

**Tech Stack:** Rust (edition 2024), `cargo test`, std only. No new dependencies, no `serde`, no crypto.

## Global Constraints

- Rust **edition 2024**; the crate currently has **no default dependencies** (only `serde`/`serde_json` behind the opt-in `seed` feature). **Phase 1 adds no dependencies** and is **not** feature-gated.
- **No `serde` / serialization in the public API** (boundary spec obligation 8). Phase 1 uses `Debug` (already derived) for golden snapshots — never `serde`.
- **No `unwrap()`/`expect()`/`panic!()` in library code.** Phase 1 changes are **test code only** (`tests/`), where `expect()` is acceptable.
- Clippy must pass: `cargo clippy -- -Dclippy::all -Dclippy::pedantic`.
- The kernel must reference **no** envelope/crypto/substrate types (obligations 6–7). Phase 1 imports only `sycore::*` and std.
- **Git policy:** per repo convention, the **human operator runs all `git` commit commands**. The commit steps below show the exact command to run; an agentic worker should surface it for the operator rather than executing it.
- Cross-seam types under test (the substrate will encode these): `event::Event`, `view::{MusicianView, OrchestraView, VenueView}` and their components, and the `entity`/`ids`/`time` types they contain.

---

## File Structure

| File | Responsibility |
|---|---|
| `tests/seam_invariants.rs` (create) | Integration tests that fortify boundary-spec obligations 1–2: `apply` determinism, cross-seam ordering stability, and a frozen-shape tripwire for cross-seam types. Pure kernel; no `seed` feature. |
| `docs/SPEC-sycore-envelope-boundary.md` (reference only) | The boundary spec these tests enforce. Not modified. |

One file, because these tests share one fixture (a fixed command script) and one purpose (guard the kernel↔substrate seam). They belong together.

---

## Phase 1 — Now: fortify the seam guarantees (executable, TDD)

### Task 1: Shared fixture + `apply` determinism

**Files:**
- Create: `tests/seam_invariants.rs`

**Interfaces:**
- Consumes (from the crate): `apply::apply(&Federation, Command) -> Result<Transition, KernelError>`; `Transition { state: Federation, events: Vec<Event>, warnings: Vec<Warning> }`; `Federation::new()`. All cross-seam types derive `Clone, Debug, PartialEq, Eq`.
- Produces (for Tasks 2–3): the test-local helpers `fn script() -> Vec<Command>` and `fn replay(&[Command]) -> (Federation, Vec<Event>)`.

- [ ] **Step 1: Write the failing test (fixture + determinism)**

Create `tests/seam_invariants.rs`:

```rust
//! Seam invariants: regression tests that fortify the guarantees the future
//! FolkEngine envelope/substrate layer depends on. See
//! `docs/SPEC-sycore-envelope-boundary.md` obligations 1 (determinism) and
//! 2 (canonical encodability). Pure kernel tests: no `seed` feature, no
//! serialization, no crypto.

use sycore::apply::apply;
use sycore::command::Command;
use sycore::entity::{Chair, EventKind, Program, Tier};
use sycore::event::Event;
use sycore::ids::{ConcertId, MusicianId, OrchestraId, VenueId};
use sycore::state::Federation;
use sycore::time::{Date, Time, TimeSlot};
use sycore::view::{view_for_musician, view_for_orchestra, view_for_venue};

/// A fixed command script exercising every cross-seam entity, in a deterministic
/// order so two independent replays are directly comparable.
fn script() -> Vec<Command> {
    vec![
        Command::RegisterMusician {
            id: MusicianId::new("M001"),
            name: "Shared".into(),
            primary_instrument: "Cello".into(),
            availability_pct: 100,
        },
        Command::FoundOrchestra {
            id: OrchestraId::new("RSO"),
            name: "Riverside".into(),
        },
        Command::RegisterVenue {
            id: VenueId::new("VEN-01"),
            name: "Main Hall".into(),
            capacity: 1000,
            stage_type: "proscenium".into(),
            has_pit: true,
            has_organ: false,
            loading_dock: true,
        },
        Command::AddToRoster {
            orchestra: OrchestraId::new("RSO"),
            musician: MusicianId::new("M001"),
            instrument: "Cello".into(),
            chair: Chair::Section,
            tier: Tier::Core,
        },
        Command::ProgramConcert {
            id: ConcertId::new("C01"),
            orchestra: OrchestraId::new("RSO"),
            series: "Masterworks".into(),
            title: "Opening Night".into(),
            program: Program {
                works: vec![],
                requires_organ: false,
                requires_pit: false,
            },
            players_required: 1,
        },
        Command::ScheduleEvent {
            concert: ConcertId::new("C01"),
            kind: EventKind::Performance,
            slot: TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180),
            venue: VenueId::new("VEN-01"),
            call_time: None,
            downbeat: None,
        },
        Command::AssignPlayer {
            concert: ConcertId::new("C01"),
            musician: MusicianId::new("M001"),
        },
    ]
}

/// Replays a script into a fresh federation, collecting the emitted event stream.
fn replay(script: &[Command]) -> (Federation, Vec<Event>) {
    let mut federation = Federation::new();
    let mut events = Vec::new();
    for command in script {
        let transition =
            apply(&federation, command.clone()).expect("script commands must all succeed");
        events.extend(transition.events);
        federation = transition.state;
    }
    (federation, events)
}

#[test]
fn apply_is_deterministic_across_replays() {
    let (federation_a, events_a) = replay(&script());
    let (federation_b, events_b) = replay(&script());
    assert_eq!(
        federation_a, federation_b,
        "identical command scripts must yield identical state (obligation 1)"
    );
    assert_eq!(
        events_a, events_b,
        "identical command scripts must yield identical event streams (obligation 1)"
    );
}
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cargo test --test seam_invariants apply_is_deterministic_across_replays`
Expected: PASS. (This invariant already holds — `apply` is pure — so the test is GREEN immediately; its value is as a regression guard. If it does not compile, fix the imports/fixture before proceeding.)

- [ ] **Step 3: Verify clippy is clean**

Run: `cargo clippy --tests -- -Dclippy::all -Dclippy::pedantic`
Expected: no warnings for `tests/seam_invariants.rs`.

- [ ] **Step 4: Commit (operator runs)**

```bash
git add tests/seam_invariants.rs && git commit -m "test: lock apply determinism as a seam invariant"
```

---

### Task 2: Cross-seam ordering stability

Guards obligation 2's "no map-iteration-order leaking into output." `Federation` uses `BTreeMap`; if someone swaps it for `HashMap`, iteration order varies per map instance, so two independently-built federations would project to *different* view orderings — and this test would fail.

**Files:**
- Modify: `tests/seam_invariants.rs` (append one test)

**Interfaces:**
- Consumes: `script()`, `replay()` from Task 1; `view_for_musician/orchestra/venue`.

- [ ] **Step 1: Write the failing test**

Append to `tests/seam_invariants.rs`:

```rust
#[test]
fn views_are_order_stable_across_independent_federations() {
    // Two independently-built, equal federations must project to identical views.
    // BTreeMap makes iteration order deterministic; a swap to HashMap would make
    // per-instance order vary and fail this assertion (obligation 2).
    let (federation_a, _) = replay(&script());
    let (federation_b, _) = replay(&script());

    assert_eq!(
        view_for_musician(&federation_a, &MusicianId::new("M001")),
        view_for_musician(&federation_b, &MusicianId::new("M001")),
    );
    assert_eq!(
        view_for_orchestra(&federation_a, &OrchestraId::new("RSO")),
        view_for_orchestra(&federation_b, &OrchestraId::new("RSO")),
    );
    assert_eq!(
        view_for_venue(&federation_a, &VenueId::new("VEN-01")),
        view_for_venue(&federation_b, &VenueId::new("VEN-01")),
    );
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test --test seam_invariants views_are_order_stable_across_independent_federations`
Expected: PASS (the invariant holds today; this is the HashMap-regression tripwire).

- [ ] **Step 3: Commit (operator runs)**

```bash
git add tests/seam_invariants.rs && git commit -m "test: guard cross-seam view ordering against map-order nondeterminism"
```

---

### Task 3: Frozen-shape tripwire for cross-seam types

A golden snapshot of a view's `Debug` rendering. A diff here means a cross-seam type's **shape** changed (field added/removed/renamed/retyped) — which would change the substrate's eventual dCBOR schema. The test forces that change to be deliberate and flagged. This is a *golden test*: the expected value is generated by running once, then pasted in.

**Files:**
- Modify: `tests/seam_invariants.rs` (append one test + one constant)

**Interfaces:**
- Consumes: `script()`, `replay()`, `view_for_musician`.

- [ ] **Step 1: Write the test with an empty golden constant (will fail)**

Append to `tests/seam_invariants.rs`:

```rust
/// GOLDEN — frozen `Debug` shape of a `MusicianView` for the fixed `script()`.
/// Regenerate deliberately: when this changes, a cross-seam type's shape changed;
/// update this constant AND note the change for the substrate's dCBOR schema.
const EXPECTED_MUSICIAN_VIEW: &str = "<fill on first run>";

#[test]
fn musician_view_shape_is_frozen() {
    let (federation, _) = replay(&script());
    let view = view_for_musician(&federation, &MusicianId::new("M001"));
    let rendered = format!("{view:#?}");
    assert_eq!(
        rendered, EXPECTED_MUSICIAN_VIEW,
        "cross-seam MusicianView shape changed — update the golden deliberately \
         and tell the substrate (obligation 2)"
    );
}
```

- [ ] **Step 2: Run to capture the actual shape**

Run: `cargo test --test seam_invariants musician_view_shape_is_frozen -- --nocapture`
Expected: FAIL. The assertion message prints `left` (the actual `{:#?}` rendering). Copy that exact multi-line string.

- [ ] **Step 3: Paste the captured rendering into the golden constant**

Replace `"<fill on first run>"` with the exact captured string. Use a raw string if it contains quotes:

```rust
const EXPECTED_MUSICIAN_VIEW: &str = r#"MusicianView {
    musician: MusicianId(
        "M001",
    ),
    calendar: [
        ...paste exact captured lines...
    ],
    own_conflicts: [],
    unavailable: [],
}"#;
```

(The exact contents come from Step 2's output — do not hand-write them; paste verbatim.)

- [ ] **Step 4: Run to verify it now passes**

Run: `cargo test --test seam_invariants musician_view_shape_is_frozen`
Expected: PASS.

- [ ] **Step 5: Full suite + clippy + commit (operator runs commit)**

Run: `cargo test --test seam_invariants` → all PASS.
Run: `cargo clippy --tests -- -Dclippy::all -Dclippy::pedantic` → clean.

```bash
git add tests/seam_invariants.rs && git commit -m "test: freeze cross-seam MusicianView shape as a dCBOR-schema tripwire"
```

> **Note (not a task):** Boundary-spec **Q3 / obligation 4** (opaque-ID discipline) is already covered by `src/ids.rs`'s `musician_id_roundtrips` / `from_str_matches_new` unit tests; a Phase 1 duplicate would be test theater, so none is added. The substrate-side half of Q3 (per-context ID pseudonymization) is roadmap item **R3** below.

---

## Phase 2 — Roadmap: blocked on substrate (NOT executable tasks)

Each item states its **entry condition** (what must exist before it can start), its **dependency** on Phase 1, and **acceptance criteria**. None can be implemented against today's repo because the FolkEngine substrate (CAS, dCBOR profile, envelope module, identity/DID layer) does not exist here.

### R1 — dCBOR canonical-encoding conformance vector (boundary spec Q1b / §4.2)

- **Entry condition:** the substrate exposes a dCBOR (Gordian profile) encoder and the `envelope` module (`bc-envelope`) per credential-envelope spec §4.3.
- **Depends on:** Phase 1 Tasks 1–3 (the kernel determinism/shape they lock are the *input* this vector encodes).
- **Work:** encode a fixed `Federation`/view (e.g., `script()`'s output) to dCBOR; build a signed envelope; produce the §4.2 two-hash vector — assert `blake3(full) != blake3(elided)`, `envelope_root(full) == envelope_root(elided)`, and that the `verifiedBy` signature verifies against both projections.
- **Lives in:** the substrate (`folkcore`), **not** SyCore. SyCore stays pure; it only supplies the deterministic input.
- **Acceptance:** the conformance vector is in the substrate's corpus and CI; SyCore's golden shape (Task 3) is referenced as the encoder's input fixture.

### R2 — First credential-shaped datum: assignment attestation (boundary spec Q4 / §4.9)

- **Entry condition:** substrate `envelope` module + issuer signing + identity→DID mapping exist; a FolkEngine **product decision** picks the first credential (recommended: *assignment attestation* — "orchestra O attests musician M is assigned to concert C", wrapping a `PlayerAssigned` fact).
- **Depends on:** R1 (encoding) and the substrate identity layer.
- **Work (substrate):** map kernel IDs to DIDs at the boundary; wrap the `PlayerAssigned` fact as a salted, signed envelope; demonstrate holder elision (e.g., reveal the assignment, elide/ salt the orchestra) with the signature surviving.
- **SyCore involvement:** **none beyond the seam contract already met.** This item validates the seam against a concrete case; it must not push any envelope/credential type into the kernel (obligations 6–7).
- **Acceptance:** an assignment attestation is issued, elided, and verified end-to-end in the substrate, using kernel facts unchanged.

### R3 — Per-context ID pseudonymization (boundary spec Q3 / §4.10 correlator edge)

- **Entry condition:** substrate identity layer with per-context wrapping; a policy decision on where unsalted root/ID exposure is acceptable vs. where per-context wrapping is required.
- **Depends on:** R2 (needs credentials in flight to correlate across).
- **Work (substrate):** wrap/pseudonymize kernel IDs per verifier context so a colluding verifier cannot correlate the same musician across disclosures.
- **SyCore involvement:** **none** — obligation 4 (opaque, structure-free IDs) already leaves the substrate free to do this; the kernel must simply continue not deriving behavior from ID structure (guarded by `ids.rs` unit tests).
- **Acceptance:** documented policy + substrate mechanism; SyCore unchanged.

---

## Self-Review

**Spec coverage** (against `docs/SPEC-sycore-envelope-boundary.md` Part 4 questions):
- Q1 down payment → Phase 1 Tasks 1–3. ✓
- Q1b (dCBOR vector) → R1. ✓
- Q2 (low-entropy inventory) → already complete in the boundary spec; no task needed (noted). ✓
- Q3 (opaque-ID discipline) → kernel half already covered by `ids.rs` tests (noted under Task 3); substrate half → R3. ✓
- Q4 (first credential datum) → R2. ✓

**Placeholder scan:** the only generated value is Task 3's golden constant, with an explicit capture-and-paste procedure (standard golden-test workflow) — not a TODO. Phase 2 items are intentionally non-executable roadmap entries with entry conditions, not placeholder tasks. No "TBD"/"add error handling"/"similar to Task N".

**Type consistency:** `apply` / `Transition.state` / `Transition.events`, `Federation::new()`, the `Command` variants, and `view_for_*` signatures match `src/apply.rs`, `src/command.rs`, `src/view.rs` as read. Cross-seam types all derive `Debug` + `PartialEq` (verified in source), so the `==` and `{:#?}` assertions compile.

---

## Execution Handoff

Phase 1 is three small, test-only tasks sharing one fixture. Phase 2 is a roadmap — do not execute it.
