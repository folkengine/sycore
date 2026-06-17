//! The only inputs that can change `Federation` state.

use crate::entity::{Chair, EventKind, Program, Tier};
use crate::ids::{ConcertId, MusicianId, OrchestraId, VenueId};
use crate::time::{Time, TimeSlot};

/// A request to change federation state. Apply via [`crate::apply::apply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    RegisterMusician {
        id: MusicianId,
        name: String,
        primary_instrument: String,
        availability_pct: u8,
    },
    RegisterVenue {
        id: VenueId,
        name: String,
        capacity: u32,
        stage_type: String,
        has_pit: bool,
        has_organ: bool,
        loading_dock: bool,
    },
    FoundOrchestra {
        id: OrchestraId,
        name: String,
    },
    AddToRoster {
        orchestra: OrchestraId,
        musician: MusicianId,
        instrument: String,
        chair: Chair,
        tier: Tier,
    },
    SetUnavailable {
        musician: MusicianId,
        slots: Vec<TimeSlot>,
    },
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
    AssignPlayer {
        concert: ConcertId,
        musician: MusicianId,
    },
    UnassignPlayer {
        concert: ConcertId,
        musician: MusicianId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_constructs_and_clones() {
        let c = Command::AssignPlayer {
            concert: ConcertId::new("C01"),
            musician: MusicianId::new("M001"),
        };
        assert_eq!(c.clone(), c);
    }
}
