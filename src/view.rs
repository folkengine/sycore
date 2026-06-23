//! Privacy-preserving per-actor projections. View types are deliberately
//! narrower than `Federation`, so over-disclosure is impossible by construction.

use crate::entity::{EventKind, RosterEntry};
use crate::ids::{ConcertId, MusicianId, OrchestraId, VenueId};
use crate::state::{Federation, count_u16};
use crate::time::{Time, TimeSlot};

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
#[must_use]
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
            let venue_name = state
                .venues
                .get(&ev.venue)
                .map(|v| v.name.clone())
                .unwrap_or_default();
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
                own_conflicts.push(SelfClash {
                    a: calendar[i].slot,
                    b: calendar[j].slot,
                });
            }
        }
    }
    let unavailable = state
        .musicians
        .get(musician)
        .map(|m| m.unavailable.clone())
        .unwrap_or_default();
    MusicianView {
        musician: musician.clone(),
        calendar,
        own_conflicts,
        unavailable,
    }
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
#[must_use]
pub fn view_for_orchestra(state: &Federation, orchestra: &OrchestraId) -> OrchestraView {
    let roster = state
        .orchestras
        .get(orchestra)
        .map(|o| o.roster.clone())
        .unwrap_or_default();

    let mut concerts = Vec::new();
    let mut coverage = Vec::new();
    for c in state
        .concerts
        .values()
        .filter(|c| &c.orchestra == orchestra)
    {
        concerts.push(ConcertSummary {
            id: c.id.clone(),
            series: c.series.clone(),
            title: c.title.clone(),
            players_required: c.players_required,
            assignments: c.assignments.clone(),
        });
        if count_u16(c.assignments.len()) < c.players_required {
            coverage.push(CoverageGap {
                concert: c.id.clone(),
                required: c.players_required,
                assigned: count_u16(c.assignments.len()),
            });
        }
    }

    // For each roster musician, surface ONLY anonymized busy windows from OTHER
    // orchestras' concerts plus their own blackout windows. No titles, venues,
    // orchestra names, or event ids cross this boundary.
    let mut blocked_slots = Vec::new();
    for entry in &roster {
        for other in state
            .concerts
            .values()
            .filter(|c| &c.orchestra != orchestra)
        {
            if other.assignments.contains(&entry.musician) {
                for ev in &other.schedule {
                    blocked_slots.push(RedactedBusy {
                        musician: entry.musician.clone(),
                        slot: ev.slot,
                    });
                }
            }
        }
        if let Some(m) = state.musicians.get(&entry.musician) {
            for slot in &m.unavailable {
                blocked_slots.push(RedactedBusy {
                    musician: entry.musician.clone(),
                    slot: *slot,
                });
            }
        }
    }

    OrchestraView {
        orchestra: orchestra.clone(),
        roster,
        concerts,
        coverage,
        blocked_slots,
    }
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
#[must_use]
pub fn view_for_venue(state: &Federation, venue: &VenueId) -> VenueView {
    let mut bookings = Vec::new();
    for c in state.concerts.values() {
        let orchestra_name = state
            .orchestras
            .get(&c.orchestra)
            .map(|o| o.name.clone())
            .unwrap_or_default();
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
    VenueView {
        venue: venue.clone(),
        bookings,
    }
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
        f = apply(
            &f,
            Command::RegisterMusician {
                id: MusicianId::new("M001"),
                name: "Shared".into(),
                primary_instrument: "Cello".into(),
                availability_pct: 100,
            },
        )
        .unwrap()
        .state;
        for (o, v) in [("RSO", "VEN-01"), ("PHIL", "VEN-02")] {
            f = apply(
                &f,
                Command::FoundOrchestra {
                    id: OrchestraId::new(o),
                    name: o.into(),
                },
            )
            .unwrap()
            .state;
            f = apply(
                &f,
                Command::RegisterVenue {
                    id: VenueId::new(v),
                    name: v.into(),
                    capacity: 1000,
                    stage_type: "p".into(),
                    has_pit: false,
                    has_organ: false,
                    loading_dock: false,
                },
            )
            .unwrap()
            .state;
            f = apply(
                &f,
                Command::AddToRoster {
                    orchestra: OrchestraId::new(o),
                    musician: MusicianId::new("M001"),
                    instrument: "Cello".into(),
                    chair: Chair::Section,
                    tier: Tier::Core,
                },
            )
            .unwrap()
            .state;
        }
        // RSO concert C01 at VEN-01, rehearsal 18:00–21:00, M001 assigned.
        f = apply(
            &f,
            Command::ProgramConcert {
                id: ConcertId::new("C01"),
                orchestra: OrchestraId::new("RSO"),
                series: "M".into(),
                title: "RSO Night".into(),
                program: Program {
                    works: vec![],
                    requires_organ: false,
                    requires_pit: false,
                },
                players_required: 1,
            },
        )
        .unwrap()
        .state;
        f = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("C01"),
                kind: crate::entity::EventKind::Performance,
                slot: TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180),
                venue: VenueId::new("VEN-01"),
                call_time: None,
                downbeat: None,
            },
        )
        .unwrap()
        .state;
        f = apply(
            &f,
            Command::AssignPlayer {
                concert: ConcertId::new("C01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap()
        .state;
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
        f = apply(
            &f,
            Command::ProgramConcert {
                id: ConcertId::new("P01"),
                orchestra: OrchestraId::new("PHIL"),
                series: "M".into(),
                title: "PHIL Secret".into(),
                program: Program {
                    works: vec![],
                    requires_organ: false,
                    requires_pit: false,
                },
                players_required: 1,
            },
        )
        .unwrap()
        .state;
        f = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("P01"),
                kind: crate::entity::EventKind::Rehearsal,
                slot: TimeSlot::new(Date::new(2024, 9, 20), Time(600), 120),
                venue: VenueId::new("VEN-02"),
                call_time: None,
                downbeat: None,
            },
        )
        .unwrap()
        .state;
        f = apply(
            &f,
            Command::AssignPlayer {
                concert: ConcertId::new("P01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap()
        .state;

        let v = view_for_orchestra(&f, &OrchestraId::new("RSO"));
        // RSO learns M001 is busy on 9/20, but the RedactedBusy struct has NO field
        // that could carry "PHIL Secret", VEN-02, or an event id.
        assert!(v.blocked_slots.iter().any(
            |b| b.musician == MusicianId::new("M001") && b.slot.date == Date::new(2024, 9, 20)
        ));
    }

    #[test]
    fn venue_sees_only_its_bookings() {
        let f = two_orchestra_clash();
        let v = view_for_venue(&f, &VenueId::new("VEN-01"));
        assert_eq!(v.bookings.len(), 1);
        assert_eq!(v.bookings[0].orchestra_name, "RSO");
    }
}
