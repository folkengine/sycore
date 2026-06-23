//! The `Federation` aggregate — the complete, immutable truth of the system.

use std::collections::BTreeMap;

use crate::entity::{Concert, Musician, Orchestra, Venue};
use crate::ids::{ConcertId, MusicianId, OrchestraId, VenueId};

/// The whole federation: shared musicians and venues, plus orchestras and the
/// concerts they program. No single actor sees all of this directly — reads go
/// through `view_for_*` projections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Federation {
    pub musicians: BTreeMap<MusicianId, Musician>,
    pub venues: BTreeMap<VenueId, Venue>,
    pub orchestras: BTreeMap<OrchestraId, Orchestra>,
    pub concerts: BTreeMap<ConcertId, Concert>,
}

impl Federation {
    /// An empty federation.
    ///
    /// # Examples
    /// ```
    /// use sycore::state::Federation;
    /// let f = Federation::new();
    /// assert!(f.musicians.is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// All concerts belonging to a given orchestra.
    ///
    /// # Examples
    /// ```
    /// use sycore::state::Federation;
    /// use sycore::ids::OrchestraId;
    /// let f = Federation::new();
    /// assert_eq!(f.concerts_of(&OrchestraId::new("RSO")).count(), 0);
    /// ```
    pub fn concerts_of<'a>(
        &'a self,
        orchestra: &'a OrchestraId,
    ) -> impl Iterator<Item = &'a Concert> {
        self.concerts
            .values()
            .filter(move |c| &c.orchestra == orchestra)
    }
}

/// Converts a collection length to `u16`, saturating at `u16::MAX`.
///
/// Player and coverage counts are represented as `u16` in the domain (e.g.
/// `players_required`), but Rust collection lengths are `usize`. This performs
/// the narrowing without an unchecked cast and without panicking: a count that
/// somehow exceeded `u16::MAX` (impossible for any realistic federation) clamps
/// rather than truncating to a wrapped, misleading value.
pub(crate) fn count_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let f = Federation::new();
        assert!(f.musicians.is_empty());
        assert!(f.venues.is_empty());
        assert!(f.orchestras.is_empty());
        assert!(f.concerts.is_empty());
    }

    #[test]
    fn count_u16_converts_and_saturates() {
        assert_eq!(count_u16(0), 0);
        assert_eq!(count_u16(60), 60);
        assert_eq!(count_u16(usize::from(u16::MAX)), u16::MAX);
        assert_eq!(count_u16(usize::from(u16::MAX) + 1), u16::MAX);
    }
}
