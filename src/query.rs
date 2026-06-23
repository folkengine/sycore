//! Read-only analyses over a `Federation`: global conflicts, coverage, and the
//! legal-assignment picker.

use std::collections::BTreeMap;

use crate::apply::assignment_conflict;
use crate::error::Conflict;
use crate::ids::{ConcertId, MusicianId};
use crate::state::{Federation, count_u16};

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
#[must_use]
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
#[must_use]
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
        if let Some(o) = orch
            && let Some(entry) = o.roster.iter().find(|r| &r.musician == m)
        {
            *by_instrument.entry(entry.instrument.clone()).or_insert(0) += 1;
        }
    }
    let assigned = count_u16(c.assignments.len());
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
#[must_use]
pub fn legal_assignments(
    state: &Federation,
    concert: &ConcertId,
    instrument: &str,
) -> Vec<MusicianId> {
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
        f = apply(
            &f,
            Command::FoundOrchestra {
                id: OrchestraId::new("RSO"),
                name: "R".into(),
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
                stage_type: "p".into(),
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
                series: "M".into(),
                title: "G".into(),
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
        for id in ["M001", "M002"] {
            f = apply(
                &f,
                Command::RegisterMusician {
                    id: MusicianId::new(id),
                    name: id.into(),
                    primary_instrument: "Violin I".into(),
                    availability_pct: 100,
                },
            )
            .unwrap()
            .state;
            f = apply(
                &f,
                Command::AddToRoster {
                    orchestra: OrchestraId::new("RSO"),
                    musician: MusicianId::new(id),
                    instrument: "Violin I".into(),
                    chair: Chair::Section,
                    tier: Tier::Core,
                },
            )
            .unwrap()
            .state;
        }
        f
    }

    #[test]
    fn coverage_reflects_assignments() {
        let mut f = setup();
        f = apply(
            &f,
            Command::AssignPlayer {
                concert: ConcertId::new("C01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap()
        .state;
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
        f = apply(
            &f,
            Command::AssignPlayer {
                concert: ConcertId::new("C01"),
                musician: MusicianId::new("M001"),
            },
        )
        .unwrap()
        .state;
        let legal = legal_assignments(&f, &ConcertId::new("C01"), "Violin I");
        assert_eq!(legal, vec![MusicianId::new("M002")]);
    }

    #[test]
    fn valid_state_has_no_conflicts() {
        let mut f = setup();
        f = apply(
            &f,
            Command::ScheduleEvent {
                concert: ConcertId::new("C01"),
                kind: EventKind::Rehearsal,
                slot: TimeSlot::new(Date::new(2024, 9, 14), Time(600), 180),
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
        assert!(conflicts(&f).is_empty());
    }
}
