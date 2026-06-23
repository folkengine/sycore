//! The transition function: `apply(&Federation, Command) -> Result<Transition, KernelError>`.

use crate::command::Command;
use crate::entity::{CalendarEvent, EventKind, Musician, Orchestra, RosterEntry, Venue};
use crate::error::{KernelError, Warning};
use crate::event::Event;
use crate::ids::{ConcertId, EventId, MusicianId, VenueId};
use crate::state::{Federation, count_u16};
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
// `apply` is intentionally one flat dispatcher over the command enum: each arm is a
// self-contained, top-to-bottom handler, and keeping them in one match keeps the full
// transition contract visible in a single place. The length is inherent to the command
// surface, not accidental complexity, so the line count is allowed deliberately.
#[allow(clippy::too_many_lines)]
pub fn apply(state: &Federation, command: Command) -> Result<Transition, KernelError> {
    let mut next = state.clone();
    let mut events = Vec::new();
    let mut warnings = Vec::new();

    match command {
        Command::RegisterMusician {
            id,
            name,
            primary_instrument,
            availability_pct,
        } => {
            if next.musicians.contains_key(&id) {
                return Err(KernelError::DuplicateId(id.to_string()));
            }
            events.push(Event::MusicianRegistered { id: id.clone() });
            next.musicians.insert(
                id.clone(),
                Musician {
                    id,
                    name,
                    primary_instrument,
                    availability_pct,
                    unavailable: vec![],
                },
            );
        }
        Command::RegisterVenue {
            id,
            name,
            capacity,
            stage_type,
            has_pit,
            has_organ,
            loading_dock,
        } => {
            if next.venues.contains_key(&id) {
                return Err(KernelError::DuplicateId(id.to_string()));
            }
            events.push(Event::VenueRegistered { id: id.clone() });
            next.venues.insert(
                id.clone(),
                Venue {
                    id,
                    name,
                    capacity,
                    stage_type,
                    has_pit,
                    has_organ,
                    loading_dock,
                },
            );
        }
        Command::FoundOrchestra { id, name } => {
            if next.orchestras.contains_key(&id) {
                return Err(KernelError::DuplicateId(id.to_string()));
            }
            events.push(Event::OrchestraFounded { id: id.clone() });
            next.orchestras.insert(
                id.clone(),
                Orchestra {
                    id,
                    name,
                    roster: vec![],
                },
            );
        }
        Command::AddToRoster {
            orchestra,
            musician,
            instrument,
            chair,
            tier,
        } => {
            let orch = next
                .orchestras
                .get_mut(&orchestra)
                .ok_or_else(|| KernelError::UnknownOrchestra(orchestra.clone()))?;
            if !next.musicians.contains_key(&musician) {
                return Err(KernelError::UnknownMusician(musician));
            }
            orch.roster.push(RosterEntry {
                musician: musician.clone(),
                instrument,
                chair,
                tier,
            });
            events.push(Event::MusicianAddedToRoster {
                orchestra,
                musician,
            });
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
        Command::ProgramConcert {
            id,
            orchestra,
            series,
            title,
            program,
            players_required,
        } => {
            if !next.orchestras.contains_key(&orchestra) {
                return Err(KernelError::UnknownOrchestra(orchestra));
            }
            if next.concerts.contains_key(&id) {
                return Err(KernelError::DuplicateId(id.to_string()));
            }
            events.push(Event::ConcertProgrammed {
                id: id.clone(),
                orchestra: orchestra.clone(),
            });
            next.concerts.insert(
                id.clone(),
                crate::entity::Concert {
                    id,
                    orchestra,
                    series,
                    title,
                    program,
                    players_required,
                    assignments: vec![],
                    schedule: vec![],
                },
            );
        }
        Command::ScheduleEvent {
            concert,
            kind,
            slot,
            venue,
            call_time,
            downbeat,
        } => {
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
                return Err(KernelError::InvalidTime {
                    reason: "zero-duration slot".into(),
                });
            }
            if let (Some(call), Some(down)) = (call_time, downbeat)
                && call > down
            {
                return Err(KernelError::InvalidTime {
                    reason: "call_time after downbeat".into(),
                });
            }

            // Venue must be free.
            for (other, other_slot) in venue_busy(&next, &venue) {
                if other_slot.overlaps(&slot) {
                    return Err(KernelError::VenueDoubleBooked {
                        venue,
                        conflicting: other,
                    });
                }
            }
            // Already-assigned musicians must not become double-booked by the new
            // event, nor be scheduled into one of their own blackout windows.
            for musician in &existing.assignments {
                for (other, other_slot) in musician_busy(&next, musician, Some(&concert)) {
                    if other_slot.overlaps(&slot) {
                        return Err(KernelError::MusicianDoubleBooked {
                            musician: musician.clone(),
                            conflicting: other,
                        });
                    }
                }
                if let Some(m) = next.musicians.get(musician) {
                    for blackout in &m.unavailable {
                        if blackout.overlaps(&slot) {
                            return Err(KernelError::MusicianUnavailable {
                                musician: musician.clone(),
                                slot,
                            });
                        }
                    }
                }
            }

            let concert_mut = next
                .concerts
                .get_mut(&concert)
                .ok_or_else(|| KernelError::UnknownConcert(concert.clone()))?;
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
            events.push(Event::EventScheduled {
                concert: concert.clone(),
                event: event_id,
            });

            // Soft warnings.
            if kind == EventKind::Performance {
                let has_rehearsal = next
                    .concerts
                    .get(&concert)
                    .is_some_and(|c| c.schedule.iter().any(|e| e.kind == EventKind::Rehearsal));
                if !has_rehearsal {
                    warnings.push(Warning::NoRehearsal {
                        concert: concert.clone(),
                    });
                }
                if let Some(v) = next.venues.get(&venue) {
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
        }
        Command::AssignPlayer { concert, musician } => {
            let Some(musician_rec) = next.musicians.get(&musician) else {
                return Err(KernelError::UnknownMusician(musician));
            };
            let availability_pct = musician_rec.availability_pct;
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
            let c_mut = next
                .concerts
                .get_mut(&concert)
                .ok_or_else(|| KernelError::UnknownConcert(concert.clone()))?;
            c_mut.assignments.push(musician.clone());
            let (required, assigned) = (c_mut.players_required, count_u16(c_mut.assignments.len()));
            events.push(Event::PlayerAssigned {
                concert: concert.clone(),
                musician: musician.clone(),
            });
            if availability_pct < 50 {
                warnings.push(Warning::LowAvailability {
                    musician,
                    availability_pct,
                });
            }
            if assigned < required {
                warnings.push(Warning::Understaffed {
                    concert,
                    required,
                    assigned,
                });
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
            let (required, assigned) = (c.players_required, count_u16(c.assignments.len()));
            events.push(Event::PlayerUnassigned {
                concert: concert.clone(),
                musician,
            });
            if assigned < required {
                warnings.push(Warning::Understaffed {
                    concert,
                    required,
                    assigned,
                });
            }
        }
    }

    Ok(Transition {
        state: next,
        events,
        warnings,
    })
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
    exclude_concert: Option<&ConcertId>,
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

/// Returns `Some(error)` if assigning `musician` to `concert` would create a
/// hard conflict (not on roster, double-booked, or unavailable). Shared by
/// `apply(AssignPlayer)` and `query::legal_assignments` so the picker and the
/// mutation use one definition of "legal".
pub(crate) fn assignment_conflict(
    state: &Federation,
    concert: &ConcertId,
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
    let Some(musician_record) = state.musicians.get(musician) else {
        return Some(KernelError::UnknownMusician(musician.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::entity::{EventKind, Program};
    use crate::ids::{ConcertId, MusicianId, OrchestraId, VenueId};
    use crate::state::Federation;
    use crate::time::{Date, Time, TimeSlot};

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
        let f = apply(&Federation::new(), reg_musician("M001"))
            .unwrap()
            .state;
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
        let f = apply(
            &f,
            Command::FoundOrchestra {
                id: OrchestraId::new("RSO"),
                name: "Riverside".into(),
            },
        )
        .unwrap()
        .state;
        let f = apply(
            &f,
            Command::RegisterVenue {
                id: VenueId::new("VEN-01"),
                name: "Concert Hall".into(),
                capacity: 1800,
                stage_type: "proscenium".into(),
                has_pit: false,
                has_organ: true,
                loading_dock: true,
            },
        )
        .unwrap()
        .state;
        assert!(f.orchestras.contains_key(&OrchestraId::new("RSO")));
        assert!(f.venues.contains_key(&VenueId::new("VEN-01")));
    }

    fn base_with_concert() -> Federation {
        let mut f = Federation::new();
        f = apply(
            &f,
            Command::FoundOrchestra {
                id: OrchestraId::new("RSO"),
                name: "Riverside".into(),
            },
        )
        .unwrap()
        .state;
        f = apply(
            &f,
            Command::RegisterVenue {
                id: VenueId::new("VEN-01"),
                name: "Hall".into(),
                capacity: 1800,
                stage_type: "proscenium".into(),
                has_pit: false,
                has_organ: true,
                loading_dock: true,
            },
        )
        .unwrap()
        .state;
        f = apply(
            &f,
            Command::ProgramConcert {
                id: ConcertId::new("C01"),
                orchestra: OrchestraId::new("RSO"),
                series: "Masterworks".into(),
                title: "Gala".into(),
                program: Program {
                    works: vec![],
                    requires_organ: false,
                    requires_pit: false,
                },
                players_required: 2,
            },
        )
        .unwrap()
        .state;
        f
    }

    fn slot(start: u16) -> TimeSlot {
        TimeSlot::new(Date::new(2024, 9, 14), Time(start), 180)
    }

    #[test]
    fn schedule_event_appends_with_derived_id() {
        let f = base_with_concert();
        let t = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("C01"),
                kind: EventKind::Rehearsal,
                slot: slot(600),
                venue: VenueId::new("VEN-01"),
                call_time: None,
                downbeat: None,
            },
        )
        .unwrap();
        let c = &t.state.concerts[&ConcertId::new("C01")];
        assert_eq!(c.schedule.len(), 1);
        assert_eq!(c.schedule[0].id, crate::ids::EventId::new("C01-E1"));
    }

    #[test]
    fn venue_double_booking_is_rejected() {
        let mut f = base_with_concert();
        f = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("C01"),
                kind: EventKind::Rehearsal,
                slot: slot(600),
                venue: VenueId::new("VEN-01"),
                call_time: None,
                downbeat: None,
            },
        )
        .unwrap()
        .state;
        let err = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("C01"),
                kind: EventKind::Performance,
                slot: slot(660),
                venue: VenueId::new("VEN-01"),
                call_time: None,
                downbeat: None,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::error::KernelError::VenueDoubleBooked { .. }
        ));
    }

    #[test]
    fn call_after_downbeat_is_invalid() {
        let f = base_with_concert();
        let err = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("C01"),
                kind: EventKind::Performance,
                slot: slot(600),
                venue: VenueId::new("VEN-01"),
                call_time: Time::from_hm(19, 0).ok(),
                downbeat: Time::from_hm(18, 0).ok(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, crate::error::KernelError::InvalidTime { .. }));
    }

    #[test]
    fn performance_without_rehearsal_warns() {
        let f = base_with_concert();
        let t = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("C01"),
                kind: EventKind::Performance,
                slot: slot(600),
                venue: VenueId::new("VEN-01"),
                call_time: None,
                downbeat: None,
            },
        )
        .unwrap();
        assert!(
            t.warnings
                .iter()
                .any(|w| matches!(w, Warning::NoRehearsal { .. }))
        );
    }

    fn roster_one(f: &Federation, id: &str) -> Federation {
        let f = apply(f, reg_musician(id)).unwrap().state;
        apply(
            &f,
            Command::AddToRoster {
                orchestra: OrchestraId::new("RSO"),
                musician: MusicianId::new(id),
                instrument: "Violin I".into(),
                chair: crate::entity::Chair::Section,
                tier: crate::entity::Tier::Core,
            },
        )
        .unwrap()
        .state
    }

    #[test]
    fn assign_requires_roster_membership() {
        let mut f = base_with_concert();
        f = apply(&f, reg_musician("M001")).unwrap().state; // in pool, not on roster
        let err = apply(
            &f,
            Command::AssignPlayer {
                concert: ConcertId::new("C01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap_err();
        assert!(matches!(err, crate::error::KernelError::NotOnRoster { .. }));
    }

    #[test]
    fn assign_succeeds_and_warns_understaffed() {
        let mut f = base_with_concert(); // players_required = 2
        f = roster_one(&f, "M001");
        let t = apply(
            &f,
            Command::AssignPlayer {
                concert: ConcertId::new("C01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap();
        assert!(
            t.state.concerts[&ConcertId::new("C01")]
                .assignments
                .contains(&MusicianId::new("M001"))
        );
        assert!(t.warnings.iter().any(|w| matches!(
            w,
            Warning::Understaffed {
                required: 2,
                assigned: 1,
                ..
            }
        )));
    }

    #[test]
    fn assign_rejects_unavailable_slot() {
        let mut f = base_with_concert();
        f = roster_one(&f, "M001");
        f = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("C01"),
                kind: EventKind::Rehearsal,
                slot: slot(600),
                venue: VenueId::new("VEN-01"),
                call_time: None,
                downbeat: None,
            },
        )
        .unwrap()
        .state;
        f = apply(
            &f,
            Command::SetUnavailable {
                musician: MusicianId::new("M001"),
                slots: vec![slot(660)],
            },
        )
        .unwrap()
        .state;
        let err = apply(
            &f,
            Command::AssignPlayer {
                concert: ConcertId::new("C01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::error::KernelError::MusicianUnavailable { .. }
        ));
    }

    #[test]
    fn schedule_rejects_event_in_assigned_players_blackout() {
        // Assign first, mark unavailable, THEN schedule into the blackout window.
        // The unavailability invariant must hold symmetrically with AssignPlayer.
        let mut f = base_with_concert();
        f = roster_one(&f, "M001");
        f = apply(
            &f,
            Command::AssignPlayer {
                concert: ConcertId::new("C01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap()
        .state;
        f = apply(
            &f,
            Command::SetUnavailable {
                musician: MusicianId::new("M001"),
                slots: vec![slot(660)],
            },
        )
        .unwrap()
        .state;
        let err = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("C01"),
                kind: EventKind::Rehearsal,
                slot: slot(600),
                venue: VenueId::new("VEN-01"),
                call_time: None,
                downbeat: None,
            },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::error::KernelError::MusicianUnavailable { .. }
        ));
    }

    #[test]
    fn unassign_removes_player() {
        let mut f = base_with_concert();
        f = roster_one(&f, "M001");
        f = apply(
            &f,
            Command::AssignPlayer {
                concert: ConcertId::new("C01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap()
        .state;
        let t = apply(
            &f,
            Command::UnassignPlayer {
                concert: ConcertId::new("C01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap();
        assert!(
            !t.state.concerts[&ConcertId::new("C01")]
                .assignments
                .contains(&MusicianId::new("M001"))
        );
    }
}
