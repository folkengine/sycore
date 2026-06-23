//! Pure, integer-backed calendar primitives. No `chrono`, no clock.

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// Constructs a date from its parts.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::Date;
    /// let d = Date { year: 2024, month: 9, day: 14 };
    /// assert_eq!(d.month, 9);
    /// ```
    #[must_use]
    pub fn new(year: u16, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }
}

/// A wall-clock time of day, stored as minutes since midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(pub u16);

impl Time {
    /// Builds a time from hours and minutes.
    ///
    /// # Errors
    /// Returns `Err` if `hour > 23` or `minute > 59`.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::Time;
    /// assert_eq!(Time::from_hm(19, 30).unwrap(), Time(1170));
    /// assert!(Time::from_hm(24, 0).is_err());
    /// ```
    pub fn from_hm(hour: u8, minute: u8) -> Result<Self, &'static str> {
        if hour > 23 || minute > 59 {
            return Err("hour must be 0..=23 and minute 0..=59");
        }
        Ok(Self(u16::from(hour) * 60 + u16::from(minute)))
    }

    /// Minutes since midnight.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::Time;
    /// assert_eq!(Time(1170).minutes(), 1170);
    /// ```
    #[must_use]
    pub fn minutes(self) -> u16 {
        self.0
    }
}

/// A bounded block of time on a single date — the unit that scheduling and
/// double-booking checks operate on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimeSlot {
    pub date: Date,
    pub start: Time,
    pub duration_min: u16,
}

impl TimeSlot {
    /// Constructs a time slot.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::{Date, Time, TimeSlot};
    /// let slot = TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180);
    /// assert_eq!(slot.duration_min, 180);
    /// ```
    #[must_use]
    pub fn new(date: Date, start: Time, duration_min: u16) -> Self {
        Self {
            date,
            start,
            duration_min,
        }
    }

    /// End time as minutes-since-midnight (may exceed 1440 for late blocks).
    ///
    /// # Examples
    /// ```
    /// use sycore::time::{Date, Time, TimeSlot};
    /// let slot = TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180);
    /// assert_eq!(slot.end_min(), 1260);
    /// ```
    #[must_use]
    pub fn end_min(self) -> u32 {
        u32::from(self.start.0) + u32::from(self.duration_min)
    }

    /// Returns `true` if two slots fall on the same date and their intervals
    /// intersect. Touching boundaries (one ends exactly when the other starts)
    /// do **not** overlap.
    ///
    /// # Examples
    /// ```
    /// use sycore::time::{Date, Time, TimeSlot};
    /// let d = Date::new(2024, 9, 14);
    /// let a = TimeSlot::new(d, Time(1080), 180); // 18:00–21:00
    /// let b = TimeSlot::new(d, Time(1200), 60);  // 20:00–21:00
    /// let c = TimeSlot::new(d, Time(1260), 60);  // 21:00–22:00
    /// assert!(a.overlaps(&b));
    /// assert!(!a.overlaps(&c)); // touch, no overlap
    /// ```
    #[must_use]
    pub fn overlaps(&self, other: &TimeSlot) -> bool {
        self.date == other.date
            && u32::from(self.start.0) < other.end_min()
            && u32::from(other.start.0) < self.end_min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> Date {
        Date::new(2024, 9, 14)
    }

    #[test]
    fn from_hm_rejects_out_of_range() {
        assert!(Time::from_hm(25, 0).is_err());
        assert!(Time::from_hm(0, 60).is_err());
        assert_eq!(Time::from_hm(0, 0).unwrap(), Time(0));
    }

    #[test]
    fn overlap_true_when_intervals_intersect() {
        let a = TimeSlot::new(d(), Time(1080), 180);
        let b = TimeSlot::new(d(), Time(1200), 60);
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
    }

    #[test]
    fn overlap_false_when_touching() {
        let a = TimeSlot::new(d(), Time(1080), 180); // ..21:00
        let b = TimeSlot::new(d(), Time(1260), 60); // 21:00..
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn overlap_false_on_different_dates() {
        let a = TimeSlot::new(Date::new(2024, 9, 14), Time(1080), 180);
        let b = TimeSlot::new(Date::new(2024, 9, 15), Time(1080), 180);
        assert!(!a.overlaps(&b));
    }
}
