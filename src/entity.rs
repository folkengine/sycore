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
            program: Program {
                works: vec![],
                requires_organ: false,
                requires_pit: false,
            },
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
