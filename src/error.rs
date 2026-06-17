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
    NotOnRoster {
        musician: MusicianId,
        orchestra: OrchestraId,
    },
    AlreadyAssigned {
        musician: MusicianId,
        concert: ConcertId,
    },
    NotAssigned {
        musician: MusicianId,
        concert: ConcertId,
    },
    /// Returned ONLY to the caller making the assignment. Carries the
    /// conflicting event so the assigner can act; never surfaced to other actors.
    MusicianDoubleBooked {
        musician: MusicianId,
        conflicting: EventId,
    },
    VenueDoubleBooked {
        venue: VenueId,
        conflicting: EventId,
    },
    MusicianUnavailable {
        musician: MusicianId,
        slot: TimeSlot,
    },
    InvalidTime {
        reason: String,
    },
    DuplicateId(String),
}

impl std::fmt::Display for KernelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KernelError::UnknownMusician(id) => write!(f, "unknown musician: {id}"),
            KernelError::UnknownVenue(id) => write!(f, "unknown venue: {id}"),
            KernelError::UnknownOrchestra(id) => write!(f, "unknown orchestra: {id}"),
            KernelError::UnknownConcert(id) => write!(f, "unknown concert: {id}"),
            KernelError::NotOnRoster {
                musician,
                orchestra,
            } => {
                write!(
                    f,
                    "musician {musician} is not on orchestra {orchestra}'s roster"
                )
            }
            KernelError::AlreadyAssigned { musician, concert } => {
                write!(
                    f,
                    "musician {musician} is already assigned to concert {concert}"
                )
            }
            KernelError::NotAssigned { musician, concert } => {
                write!(
                    f,
                    "musician {musician} is not assigned to concert {concert}"
                )
            }
            KernelError::MusicianDoubleBooked {
                musician,
                conflicting,
            } => {
                write!(
                    f,
                    "musician {musician} is double-booked against event {conflicting}"
                )
            }
            KernelError::VenueDoubleBooked { venue, conflicting } => {
                write!(
                    f,
                    "venue {venue} is double-booked against event {conflicting}"
                )
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
    Understaffed {
        concert: ConcertId,
        required: u16,
        assigned: u16,
    },
    LowAvailability {
        musician: MusicianId,
        availability_pct: u8,
    },
    VenueCapabilityMismatch {
        venue: VenueId,
        capability: String,
    },
    NoRehearsal {
        concert: ConcertId,
    },
}

/// A conflict discovered by the global `conflicts` query (kernel-internal view;
/// never surfaced raw to any single actor).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    MusicianDoubleBooked {
        musician: MusicianId,
        events: (EventId, EventId),
    },
    VenueDoubleBooked {
        venue: VenueId,
        events: (EventId, EventId),
    },
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
        let w = Warning::Understaffed {
            concert: ConcertId::new("C01"),
            required: 60,
            assigned: 58,
        };
        assert_eq!(
            w,
            Warning::Understaffed {
                concert: ConcertId::new("C01"),
                required: 60,
                assigned: 58
            }
        );
    }
}
