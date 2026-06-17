//! Type-safe string identifiers.
//!
//! Each entity gets its own newtype so the compiler prevents mixing a
//! [`VenueId`] where a [`ConcertId`] is expected.

/// Defines a string-newtype identifier with the standard derives and helpers.
macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Wraps a raw id string.
            ///
            /// # Examples
            /// ```
            /// use sycore::ids::MusicianId;
            /// let id = MusicianId::new("M001");
            /// assert_eq!(id.as_str(), "M001");
            /// ```
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// Borrows the underlying id string.
            ///
            /// # Examples
            /// ```
            /// use sycore::ids::VenueId;
            /// assert_eq!(VenueId::new("VEN-01").as_str(), "VEN-01");
            /// ```
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

define_id!(
    /// Identifies a musician in the shared global pool.
    MusicianId
);
define_id!(
    /// Identifies an orchestra.
    OrchestraId
);
define_id!(
    /// Identifies a shared, bookable venue.
    VenueId
);
define_id!(
    /// Identifies a concert programmed by an orchestra.
    ConcertId
);
define_id!(
    /// Identifies a single calendar event (rehearsal or performance).
    EventId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musician_id_roundtrips() {
        let id = MusicianId::new("M001");
        assert_eq!(id.as_str(), "M001");
        assert_eq!(id.to_string(), "M001");
    }

    #[test]
    fn ids_of_different_types_are_distinct_types() {
        // This compiles only because they are separate types; equality is within-type.
        let a = ConcertId::from("C01");
        let b = ConcertId::from("C01");
        assert_eq!(a, b);
    }

    #[test]
    fn from_str_matches_new() {
        assert_eq!(VenueId::from("VEN-01"), VenueId::new("VEN-01"));
    }
}
