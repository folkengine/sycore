//! Typed domain events emitted by successful transitions.

use crate::ids::{ConcertId, EventId, MusicianId, OrchestraId, VenueId};

/// A record of what changed in a successful `apply`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    MusicianRegistered {
        id: MusicianId,
    },
    VenueRegistered {
        id: VenueId,
    },
    OrchestraFounded {
        id: OrchestraId,
    },
    MusicianAddedToRoster {
        orchestra: OrchestraId,
        musician: MusicianId,
    },
    UnavailabilitySet {
        musician: MusicianId,
        count: usize,
    },
    ConcertProgrammed {
        id: ConcertId,
        orchestra: OrchestraId,
    },
    EventScheduled {
        concert: ConcertId,
        event: EventId,
    },
    PlayerAssigned {
        concert: ConcertId,
        musician: MusicianId,
    },
    PlayerUnassigned {
        concert: ConcertId,
        musician: MusicianId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_equality() {
        let a = Event::PlayerAssigned {
            concert: ConcertId::new("C01"),
            musician: MusicianId::new("M001"),
        };
        let b = Event::PlayerAssigned {
            concert: ConcertId::new("C01"),
            musician: MusicianId::new("M001"),
        };
        assert_eq!(a, b);
    }
}
