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
}
