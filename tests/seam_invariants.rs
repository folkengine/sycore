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

/// GOLDEN — frozen `Debug` shape of a `MusicianView` for the fixed `script()`.
/// Regenerate deliberately: when this changes, a cross-seam type's shape changed;
/// update this constant AND note the change for the substrate's dCBOR schema.
const EXPECTED_MUSICIAN_VIEW: &str = r#"MusicianView {
    musician: MusicianId(
        "M001",
    ),
    calendar: [
        CalendarItem {
            orchestra_name: "Riverside",
            concert_title: "Opening Night",
            kind: Performance,
            slot: TimeSlot {
                date: Date {
                    year: 2024,
                    month: 9,
                    day: 14,
                },
                start: Time(
                    1080,
                ),
                duration_min: 180,
            },
            venue_name: "Main Hall",
            call_time: None,
            downbeat: None,
        },
    ],
    own_conflicts: [],
    unavailable: [],
}"#;

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
