#[cfg(all(feature = "formatting", feature = "alloc"))]
use alloc::string::String;
#[cfg(feature = "parsing")]
use core::convert::TryInto;
use core::fmt;
use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration as StdDuration;

#[cfg(any(feature = "formatting", feature = "parsing"))]
use crate::format_description::FormatDescription;
#[cfg(feature = "formatting")]
use crate::format_description::{modifier, Component};
#[cfg(feature = "parsing")]
use crate::parsing::Parsed;
use crate::util::{days_in_year, days_in_year_month, is_leap_year, weeks_in_year};
use crate::{error, Duration, PrimitiveDateTime, Time, Weekday};

/// The minimum valid year.
#[cfg(feature = "large-dates")]
pub(crate) const MIN_YEAR: i32 = -999_999;
/// The maximum valid year.
#[cfg(feature = "large-dates")]
pub(crate) const MAX_YEAR: i32 = 999_999;

/// The minimum valid year.
#[cfg(not(feature = "large-dates"))]
pub(crate) const MIN_YEAR: i32 = -9999;
/// The maximum valid year.
#[cfg(not(feature = "large-dates"))]
pub(crate) const MAX_YEAR: i32 = 9999;

/// Date in the proleptic Gregorian calendar.
///
/// By default, years between ±9999 inclusive are representable. This can be expanded to ±999,999
/// inclusive by enabling the `large-dates` crate feature. Doing so has some performance
/// implications, and introduces some ambiguities when parsing.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Date {
    /// Bitpacked field containing both the year and ordinal.
    // |     xx     | xxxxxxxxxxxxxxxxxxxxx | xxxxxxxxx |
    // |   2 bits   |        21 bits        |  9 bits   |
    // | unassigned |         year          |  ordinal  |
    // The year is 15 bits when `large-dates` is not enabled.
    pub(crate) value: i32,
}

impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.debug_struct("Date")
            .field("year", &self.year())
            .field("ordinal", &self.ordinal())
            .finish()
    }
}

impl Date {
    /// The minimum valid `Date`.
    ///
    /// The value of this may vary depending on the feature flags enabled.
    pub const MIN: Self = Self::from_ordinal_date_unchecked(MIN_YEAR, 1);

    /// The maximum valid `Date`.
    ///
    /// The value of this may vary depending on the feature flags enabled.
    pub const MAX: Self = Self::from_ordinal_date_unchecked(MAX_YEAR, days_in_year(MAX_YEAR));

    /// Construct a `Date` from the year and ordinal values, the validity of which must be
    /// guaranteed by the caller.
    #[doc(hidden)]
    pub const fn from_ordinal_date_unchecked(year: i32, ordinal: u16) -> Self {
        Self {
            value: (year << 9) | ordinal as i32,
        }
    }

    /// Attempt to create a `Date` from the year, month, and day.
    ///
    /// ```rust
    /// # use time::Date;
    /// assert!(Date::from_calendar_date(2019, 1, 1).is_ok());
    /// assert!(Date::from_calendar_date(2019, 12, 31).is_ok());
    /// ```
    ///
    /// ```rust
    /// # use time::Date;
    /// assert!(Date::from_calendar_date(2019, 2, 29).is_err()); // 2019 isn't a leap year.
    /// ```
    pub const fn from_calendar_date(
        year: i32,
        month: u8,
        day: u8,
    ) -> Result<Self, error::ComponentRange> {
        /// Cumulative days through the beginning of a month in both common and leap years.
        const DAYS_CUMULATIVE_COMMON_LEAP: [[u16; 12]; 2] = [
            [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334],
            [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335],
        ];

        ensure_value_in_range!(year in MIN_YEAR => MAX_YEAR);
        ensure_value_in_range!(month in 1 => 12);
        ensure_value_in_range!(day conditionally in 1 => days_in_year_month(year, month));

        Ok(Self::from_ordinal_date_unchecked(
            year,
            DAYS_CUMULATIVE_COMMON_LEAP[is_leap_year(year) as usize][month as usize - 1]
                + day as u16,
        ))
    }

    /// Attempt to create a `Date` from the year and ordinal day number.
    ///
    /// ```rust
    /// # use time::Date;
    /// assert!(Date::from_ordinal_date(2019, 1).is_ok());
    /// assert!(Date::from_ordinal_date(2019, 365).is_ok());
    /// ```
    ///
    /// ```rust
    /// # use time::Date;
    /// assert!(Date::from_ordinal_date(2019, 366).is_err()); // 2019 isn't a leap year.
    /// ```
    pub const fn from_ordinal_date(year: i32, ordinal: u16) -> Result<Self, error::ComponentRange> {
        ensure_value_in_range!(year in MIN_YEAR => MAX_YEAR);
        ensure_value_in_range!(ordinal conditionally in 1 => days_in_year(year));
        Ok(Self::from_ordinal_date_unchecked(year, ordinal))
    }

    /// Attempt to create a `Date` from the ISO year, week, and weekday.
    ///
    /// ```rust
    /// # use time::{Date, Weekday::*};
    /// assert!(Date::from_iso_week_date(2019, 1, Monday).is_ok());
    /// assert!(Date::from_iso_week_date(2019, 1, Tuesday).is_ok());
    /// assert!(Date::from_iso_week_date(2020, 53, Friday).is_ok());
    /// ```
    ///
    /// ```rust
    /// # use time::{Date, Weekday::*};
    /// assert!(Date::from_iso_week_date(2019, 53, Monday).is_err()); // 2019 doesn't have 53 weeks.
    /// ```
    pub const fn from_iso_week_date(
        year: i32,
        week: u8,
        weekday: Weekday,
    ) -> Result<Self, error::ComponentRange> {
        ensure_value_in_range!(year in MIN_YEAR => MAX_YEAR);
        ensure_value_in_range!(week conditionally in 1 => weeks_in_year(year));

        let (ordinal, overflow) = (week as u16 * 7 + weekday.number_from_monday() as u16)
            .overflowing_sub({
                let adj_year = year - 1;
                let rem = (adj_year + adj_year / 4 - adj_year / 100 + adj_year / 400 + 3) % 7;
                if rem < 0 {
                    (rem + 11) as _
                } else {
                    (rem + 4) as _
                }
            });

        if overflow || ordinal == 0 {
            return Ok(Self::from_ordinal_date_unchecked(
                year - 1,
                ordinal.wrapping_add(days_in_year(year - 1)),
            ));
        }

        let days_in_cur_year = days_in_year(year);
        if ordinal > days_in_cur_year {
            Ok(Self::from_ordinal_date_unchecked(
                year + 1,
                ordinal - days_in_cur_year,
            ))
        } else {
            Ok(Self::from_ordinal_date_unchecked(year, ordinal))
        }
    }

    /// Get the year of the date.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("2019-01-01").year(), 2019);
    /// assert_eq!(date!("2019-12-31").year(), 2019);
    /// assert_eq!(date!("2020-01-01").year(), 2020);
    /// ```
    pub const fn year(self) -> i32 {
        self.value >> 9
    }

    /// Get the month.
    ///
    /// The returned value will always be in the range `1..=12`.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("2019-01-01").month(), 1);
    /// assert_eq!(date!("2019-12-31").month(), 12);
    /// ```
    pub const fn month(self) -> u8 {
        self.month_day().0
    }

    /// Get the day of the month. If fetching both the month and day, it is more efficient to use
    /// [`Date::month_day`].
    ///
    /// The returned value will always be in the range `1..=31`.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("2019-01-01").day(), 1);
    /// assert_eq!(date!("2019-12-31").day(), 31);
    /// ```
    pub const fn day(self) -> u8 {
        self.month_day().1
    }

    /// Get the month and day. This is more efficient than fetching the components individually.
    // For whatever reason, rustc has difficulty optimizing this function. It's significantly faster
    // to write the statements out by hand.
    pub(crate) const fn month_day(self) -> (u8, u8) {
        /// The number of days up to and including the given month. Common years
        /// are first, followed by leap years.
        const CUMULATIVE_DAYS_IN_MONTH_COMMON_LEAP: [[u16; 11]; 2] = [
            [31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334],
            [31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335],
        ];

        let days = CUMULATIVE_DAYS_IN_MONTH_COMMON_LEAP[is_leap_year(self.year()) as usize];
        let ordinal = self.ordinal();

        if ordinal > days[10] {
            (12, (ordinal - days[10]) as _)
        } else if ordinal > days[9] {
            (11, (ordinal - days[9]) as _)
        } else if ordinal > days[8] {
            (10, (ordinal - days[8]) as _)
        } else if ordinal > days[7] {
            (9, (ordinal - days[7]) as _)
        } else if ordinal > days[6] {
            (8, (ordinal - days[6]) as _)
        } else if ordinal > days[5] {
            (7, (ordinal - days[5]) as _)
        } else if ordinal > days[4] {
            (6, (ordinal - days[4]) as _)
        } else if ordinal > days[3] {
            (5, (ordinal - days[3]) as _)
        } else if ordinal > days[2] {
            (4, (ordinal - days[2]) as _)
        } else if ordinal > days[1] {
            (3, (ordinal - days[1]) as _)
        } else if ordinal > days[0] {
            (2, (ordinal - days[0]) as _)
        } else {
            (1, ordinal as _)
        }
    }

    /// Get the day of the year.
    ///
    /// The returned value will always be in the range `1..=366` (`1..=365` for common years).
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("2019-01-01").ordinal(), 1);
    /// assert_eq!(date!("2019-12-31").ordinal(), 365);
    /// ```
    pub const fn ordinal(self) -> u16 {
        (self.value & 0x1FF) as _
    }

    /// Get the ISO 8601 year and week number.
    pub(crate) const fn iso_year_week(self) -> (i32, u8) {
        let (year, ordinal) = self.to_ordinal_date();

        match ((ordinal + 10 - self.weekday().number_from_monday() as u16) / 7) as _ {
            0 => (year - 1, weeks_in_year(year - 1)),
            53 if weeks_in_year(year) == 52 => (year + 1, 1),
            week => (year, week),
        }
    }

    /// Get the ISO week number.
    ///
    /// The returned value will always be in the range `1..=53`.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("2019-01-01").iso_week(), 1);
    /// assert_eq!(date!("2019-10-04").iso_week(), 40);
    /// assert_eq!(date!("2020-01-01").iso_week(), 1);
    /// assert_eq!(date!("2020-12-31").iso_week(), 53);
    /// assert_eq!(date!("2021-01-01").iso_week(), 53);
    /// ```
    pub const fn iso_week(self) -> u8 {
        self.iso_year_week().1
    }

    /// Get the week number where week 1 begins on the first Sunday.
    ///
    /// The returned value will always be in the range `0..=53`.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("2019-01-01").sunday_based_week(), 0);
    /// assert_eq!(date!("2020-01-01").sunday_based_week(), 0);
    /// assert_eq!(date!("2020-12-31").sunday_based_week(), 52);
    /// assert_eq!(date!("2021-01-01").sunday_based_week(), 0);
    /// ```
    pub const fn sunday_based_week(self) -> u8 {
        ((self.ordinal() as i16 - self.weekday().number_days_from_sunday() as i16 + 6) / 7) as _
    }

    /// Get the week number where week 1 begins on the first Monday.
    ///
    /// The returned value will always be in the range `0..=53`.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("2019-01-01").monday_based_week(), 0);
    /// assert_eq!(date!("2020-01-01").monday_based_week(), 0);
    /// assert_eq!(date!("2020-12-31").monday_based_week(), 52);
    /// assert_eq!(date!("2021-01-01").monday_based_week(), 0);
    /// ```
    pub const fn monday_based_week(self) -> u8 {
        ((self.ordinal() as i16 - self.weekday().number_days_from_monday() as i16 + 6) / 7) as _
    }

    /// Get the year, month, and day.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("2019-01-01").to_calendar_date(), (2019, 1, 1));
    /// ```
    pub const fn to_calendar_date(self) -> (i32, u8, u8) {
        let (month, day) = self.month_day();
        (self.year(), month, day)
    }

    /// Get the year and ordinal day number.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("2019-01-01").to_ordinal_date(), (2019, 1));
    /// ```
    pub const fn to_ordinal_date(self) -> (i32, u16) {
        (self.year(), self.ordinal())
    }

    /// Get the ISO 8601 year, week number, and weekday.
    ///
    /// ```rust
    /// # use time::{Weekday::*, macros::date};
    /// assert_eq!(date!("2019-01-01").to_iso_week_date(), (2019, 1, Tuesday));
    /// assert_eq!(date!("2019-10-04").to_iso_week_date(), (2019, 40, Friday));
    /// assert_eq!(date!("2020-01-01").to_iso_week_date(), (2020, 1, Wednesday));
    /// assert_eq!(date!("2020-12-31").to_iso_week_date(), (2020, 53, Thursday));
    /// assert_eq!(date!("2021-01-01").to_iso_week_date(), (2020, 53, Friday));
    /// ```
    pub const fn to_iso_week_date(self) -> (i32, u8, Weekday) {
        let (year, ordinal) = self.to_ordinal_date();
        let weekday = self.weekday();

        match ((ordinal + 10 - self.weekday().number_from_monday() as u16) / 7) as _ {
            0 => (year - 1, weeks_in_year(year - 1), weekday),
            53 if weeks_in_year(year) == 52 => (year + 1, 1, weekday),
            week => (year, week, weekday),
        }
    }

    /// Get the weekday.
    ///
    /// ```rust
    /// # use time::{Weekday::*, macros::date};
    /// assert_eq!(date!("2019-01-01").weekday(), Tuesday);
    /// assert_eq!(date!("2019-02-01").weekday(), Friday);
    /// assert_eq!(date!("2019-03-01").weekday(), Friday);
    /// assert_eq!(date!("2019-04-01").weekday(), Monday);
    /// assert_eq!(date!("2019-05-01").weekday(), Wednesday);
    /// assert_eq!(date!("2019-06-01").weekday(), Saturday);
    /// assert_eq!(date!("2019-07-01").weekday(), Monday);
    /// assert_eq!(date!("2019-08-01").weekday(), Thursday);
    /// assert_eq!(date!("2019-09-01").weekday(), Sunday);
    /// assert_eq!(date!("2019-10-01").weekday(), Tuesday);
    /// assert_eq!(date!("2019-11-01").weekday(), Friday);
    /// assert_eq!(date!("2019-12-01").weekday(), Sunday);
    /// ```
    pub const fn weekday(self) -> Weekday {
        match self.to_julian_day() % 7 {
            -6 | 1 => Weekday::Tuesday,
            -5 | 2 => Weekday::Wednesday,
            -4 | 3 => Weekday::Thursday,
            -3 | 4 => Weekday::Friday,
            -2 | 5 => Weekday::Saturday,
            -1 | 6 => Weekday::Sunday,
            _ => Weekday::Monday,
        }
    }

    /// Get the next calendar date.
    ///
    /// ```rust
    /// # use time::{Date, macros::date};
    /// assert_eq!(date!("2019-01-01").next_day(), Some(date!("2019-01-02")));
    /// assert_eq!(date!("2019-01-31").next_day(), Some(date!("2019-02-01")));
    /// assert_eq!(date!("2019-12-31").next_day(), Some(date!("2020-01-01")));
    /// assert_eq!(Date::MAX.next_day(), None);
    /// ```
    pub const fn next_day(self) -> Option<Self> {
        if self.ordinal() == 366 || (self.ordinal() == 365 && !is_leap_year(self.year())) {
            if self.value == Self::MAX.value {
                None
            } else {
                Some(Self::from_ordinal_date_unchecked(self.year() + 1, 1))
            }
        } else {
            Some(Self {
                value: self.value + 1,
            })
        }
    }

    /// Get the previous calendar date.
    ///
    /// ```rust
    /// # use time::{Date, macros::date};
    /// assert_eq!(
    ///     date!("2019-01-02").previous_day(),
    ///     Some(date!("2019-01-01"))
    /// );
    /// assert_eq!(
    ///     date!("2019-02-01").previous_day(),
    ///     Some(date!("2019-01-31"))
    /// );
    /// assert_eq!(
    ///     date!("2020-01-01").previous_day(),
    ///     Some(date!("2019-12-31"))
    /// );
    /// assert_eq!(Date::MIN.previous_day(), None);
    /// ```
    pub const fn previous_day(self) -> Option<Self> {
        if self.ordinal() != 1 {
            Some(Self {
                value: self.value - 1,
            })
        } else if self.value == Self::MIN.value {
            None
        } else {
            Some(Self::from_ordinal_date_unchecked(
                self.year() - 1,
                days_in_year(self.year() - 1),
            ))
        }
    }

    /// Get the Julian day for the date.
    ///
    /// The algorithm to perform this conversion is derived from one provided by Peter Baum; it is
    /// freely available [here](https://www.researchgate.net/publication/316558298_Date_Algorithms).
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert_eq!(date!("-4713-11-24").to_julian_day(), 0);
    /// assert_eq!(date!("2000-01-01").to_julian_day(), 2_451_545);
    /// assert_eq!(date!("2019-01-01").to_julian_day(), 2_458_485);
    /// assert_eq!(date!("2019-12-31").to_julian_day(), 2_458_849);
    /// ```
    pub const fn to_julian_day(self) -> i32 {
        let year = self.year() - 1;
        let ordinal = self.ordinal() as i32;

        ordinal + 365 * year + div_floor!(year, 4) - div_floor!(year, 100)
            + div_floor!(year, 400)
            + 1_721_425
    }

    /// Create a `Date` from the Julian day.
    ///
    /// The algorithm to perform this conversion is derived from one provided by Peter Baum; it is
    /// freely available [here](https://www.researchgate.net/publication/316558298_Date_Algorithms).
    ///
    /// ```rust
    /// # use time::{Date, macros::date};
    /// assert_eq!(Date::from_julian_day(0), Ok(date!("-4713-11-24")));
    /// assert_eq!(Date::from_julian_day(2_451_545), Ok(date!("2000-01-01")));
    /// assert_eq!(Date::from_julian_day(2_458_485), Ok(date!("2019-01-01")));
    /// assert_eq!(Date::from_julian_day(2_458_849), Ok(date!("2019-12-31")));
    /// ```
    #[cfg_attr(__time_03_docs, doc(alias = "from_julian_date"))]
    pub const fn from_julian_day(julian_day: i32) -> Result<Self, error::ComponentRange> {
        ensure_value_in_range!(
            julian_day in Self::MIN.to_julian_day() => Self::MAX.to_julian_day()
        );
        Ok(Self::from_julian_day_unchecked(julian_day))
    }

    /// Create a `Date` from the Julian day.
    ///
    /// This does not check the validity of the provided Julian day, and as such may result in an
    /// internally invalid value.
    #[cfg_attr(__time_03_docs, doc(alias = "from_julian_date_unchecked"))]
    pub(crate) const fn from_julian_day_unchecked(julian_day: i32) -> Self {
        #![allow(trivial_numeric_casts)] // cast depends on type alias

        /// A type that is either `i32` or `i64`. This subtle difference allows for optimization
        /// based on the valid values.
        #[cfg(feature = "large-dates")]
        type MaybeWidened = i64;
        #[allow(clippy::missing_docs_in_private_items)]
        #[cfg(not(feature = "large-dates"))]
        type MaybeWidened = i32;

        // To avoid a potential overflow, the value may need to be widened for some arithmetic.

        let z = julian_day - 1_721_119;
        let g = 100 * z as MaybeWidened - 25;
        let a = (g / 3_652_425) as i32;
        let b = a - a / 4;
        let mut year = div_floor!(100 * b as MaybeWidened + g, 36525) as i32;
        let mut ordinal = (b + z - div_floor!(36525 * year as MaybeWidened, 100) as i32) as _;

        if is_leap_year(year) {
            ordinal += 60;
            cascade!(ordinal in 1..367 => year);
        } else {
            ordinal += 59;
            cascade!(ordinal in 1..366 => year);
        }

        Self::from_ordinal_date_unchecked(year, ordinal)
    }
}

/// Methods to add a [`Time`] component, resulting in a [`PrimitiveDateTime`].
impl Date {
    /// Create a [`PrimitiveDateTime`] using the existing date. The [`Time`] component will be set
    /// to midnight.
    ///
    /// ```rust
    /// # use time::macros::{date, datetime};
    /// assert_eq!(date!("1970-01-01").midnight(), datetime!("1970-01-01 0:00"));
    /// ```
    pub const fn midnight(self) -> PrimitiveDateTime {
        PrimitiveDateTime::new(self, Time::MIDNIGHT)
    }

    /// Create a [`PrimitiveDateTime`] using the existing date and the provided [`Time`].
    ///
    /// ```rust
    /// # use time::macros::{date, datetime, time};
    /// assert_eq!(
    ///     date!("1970-01-01").with_time(time!("0:00")),
    ///     datetime!("1970-01-01 0:00"),
    /// );
    /// ```
    pub const fn with_time(self, time: Time) -> PrimitiveDateTime {
        PrimitiveDateTime::new(self, time)
    }

    /// Attempt to create a [`PrimitiveDateTime`] using the existing date and the provided time.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert!(date!("1970-01-01").with_hms(0, 0, 0).is_ok());
    /// assert!(date!("1970-01-01").with_hms(24, 0, 0).is_err());
    /// ```
    pub const fn with_hms(
        self,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> Result<PrimitiveDateTime, error::ComponentRange> {
        Ok(PrimitiveDateTime::new(
            self,
            const_try!(Time::from_hms(hour, minute, second)),
        ))
    }

    /// Attempt to create a [`PrimitiveDateTime`] using the existing date and the provided time.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert!(date!("1970-01-01").with_hms_milli(0, 0, 0, 0).is_ok());
    /// assert!(date!("1970-01-01").with_hms_milli(24, 0, 0, 0).is_err());
    /// ```
    pub const fn with_hms_milli(
        self,
        hour: u8,
        minute: u8,
        second: u8,
        millisecond: u16,
    ) -> Result<PrimitiveDateTime, error::ComponentRange> {
        Ok(PrimitiveDateTime::new(
            self,
            const_try!(Time::from_hms_milli(hour, minute, second, millisecond)),
        ))
    }

    /// Attempt to create a [`PrimitiveDateTime`] using the existing date and the provided time.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert!(date!("1970-01-01").with_hms_micro(0, 0, 0, 0).is_ok());
    /// assert!(date!("1970-01-01").with_hms_micro(24, 0, 0, 0).is_err());
    /// ```
    pub const fn with_hms_micro(
        self,
        hour: u8,
        minute: u8,
        second: u8,
        microsecond: u32,
    ) -> Result<PrimitiveDateTime, error::ComponentRange> {
        Ok(PrimitiveDateTime::new(
            self,
            const_try!(Time::from_hms_micro(hour, minute, second, microsecond)),
        ))
    }

    /// Attempt to create a [`PrimitiveDateTime`] using the existing date and the provided time.
    ///
    /// ```rust
    /// # use time::macros::date;
    /// assert!(date!("1970-01-01").with_hms_nano(0, 0, 0, 0).is_ok());
    /// assert!(date!("1970-01-01").with_hms_nano(24, 0, 0, 0).is_err());
    /// ```
    pub const fn with_hms_nano(
        self,
        hour: u8,
        minute: u8,
        second: u8,
        nanosecond: u32,
    ) -> Result<PrimitiveDateTime, error::ComponentRange> {
        Ok(PrimitiveDateTime::new(
            self,
            const_try!(Time::from_hms_nano(hour, minute, second, nanosecond)),
        ))
    }
}

#[cfg(feature = "formatting")]
#[cfg_attr(__time_03_docs, doc(cfg(feature = "formatting")))]
impl Date {
    /// Format the `Date` using the provided format description. The formatted value will be output
    /// to the provided writer. The format description will typically be parsed by using
    /// [`FormatDescription::parse`].
    pub fn format_into(
        self,
        output: &mut impl fmt::Write,
        description: &FormatDescription<'_>,
    ) -> Result<(), error::Format> {
        description.format_into(output, Some(self), None, None)
    }

    /// Format the `Date` using the provided format description. The format description will
    /// typically be parsed by using [`FormatDescription::parse`].
    ///
    /// ```rust
    /// # use time::{format_description::FormatDescription, macros::date};
    /// let format = FormatDescription::parse("[year]-[month repr:numerical]-[day]")?;
    /// assert_eq!(date!("2020-01-02").format(&format)?, "2020-01-02");
    /// # Ok::<_, time::Error>(())
    /// ```
    #[cfg(feature = "alloc")]
    #[cfg_attr(__time_03_docs, doc(cfg(feature = "alloc")))]
    pub fn format(self, description: &FormatDescription<'_>) -> Result<String, error::Format> {
        let mut s = String::new();
        self.format_into(&mut s, description)?;
        Ok(s)
    }
}

#[cfg(feature = "parsing")]
#[cfg_attr(__time_03_docs, doc(cfg(feature = "parsing")))]
impl Date {
    /// Parse a `Date` from the input using the provided format description. The format description
    /// will typically be parsed by using [`FormatDescription::parse`].
    ///
    /// ```rust
    /// # use time::{format_description::FormatDescription, macros::date, Date};
    /// let format = FormatDescription::parse("[year]-[month repr:numerical]-[day]")?;
    /// assert_eq!(Date::parse("2020-01-02", &format)?, date!("2020-01-02"));
    /// # Ok::<_, time::Error>(())
    /// ```
    pub fn parse(input: &str, description: &FormatDescription<'_>) -> Result<Self, error::Parse> {
        Ok(Parsed::parse_from_description(input, description)?.try_into()?)
    }
}

#[cfg(feature = "formatting")]
#[cfg_attr(__time_03_docs, doc(cfg(feature = "formatting")))]
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.format_into(
            f,
            &FormatDescription::BorrowedCompound(&[
                FormatDescription::Component(Component::Year(modifier::Year {
                    padding: modifier::Padding::Zero,
                    repr: modifier::YearRepr::Full,
                    iso_week_based: false,
                    sign_is_mandatory: false,
                })),
                FormatDescription::Literal("-"),
                FormatDescription::Component(Component::Month(modifier::Month {
                    padding: modifier::Padding::Zero,
                    repr: modifier::MonthRepr::Numerical,
                })),
                FormatDescription::Literal("-"),
                FormatDescription::Component(Component::Day(modifier::Day {
                    padding: modifier::Padding::Zero,
                })),
            ]),
        ) {
            Ok(()) => Ok(()),
            Err(error::Format::StdFmt) => Err(fmt::Error),
            Err(error::Format::InsufficientTypeInformation { .. }) => {
                unreachable!("All components used only require a `Date`")
            }
        }
    }
}

impl Add<Duration> for Date {
    type Output = Self;

    fn add(self, duration: Duration) -> Self::Output {
        Self::from_julian_day(self.to_julian_day() + duration.whole_days() as i32)
            .expect("overflow adding duration to date")
    }
}

impl Add<StdDuration> for Date {
    type Output = Self;

    fn add(self, duration: StdDuration) -> Self::Output {
        Self::from_julian_day(self.to_julian_day() + (duration.as_secs() / 86_400) as i32)
            .expect("overflow adding duration to date")
    }
}

impl AddAssign<Duration> for Date {
    fn add_assign(&mut self, duration: Duration) {
        *self = *self + duration;
    }
}

impl AddAssign<StdDuration> for Date {
    fn add_assign(&mut self, duration: StdDuration) {
        *self = *self + duration;
    }
}

impl Sub<Duration> for Date {
    type Output = Self;

    fn sub(self, duration: Duration) -> Self::Output {
        self + -duration
    }
}

impl Sub<StdDuration> for Date {
    type Output = Self;

    fn sub(self, duration: StdDuration) -> Self::Output {
        Self::from_julian_day(self.to_julian_day() - (duration.as_secs() / 86_400) as i32)
            .expect("overflow subtracting duration from date")
    }
}

impl SubAssign<Duration> for Date {
    fn sub_assign(&mut self, duration: Duration) {
        *self = *self - duration;
    }
}

impl SubAssign<StdDuration> for Date {
    fn sub_assign(&mut self, duration: StdDuration) {
        *self = *self - duration;
    }
}

impl Sub<Date> for Date {
    type Output = Duration;

    fn sub(self, other: Self) -> Self::Output {
        Duration::days((self.to_julian_day() - other.to_julian_day()) as _)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_days_in_year_month() {
        // Common year
        assert_eq!(days_in_year_month(2019, 1), 31);
        assert_eq!(days_in_year_month(2019, 2), 28);
        assert_eq!(days_in_year_month(2019, 3), 31);
        assert_eq!(days_in_year_month(2019, 4), 30);
        assert_eq!(days_in_year_month(2019, 5), 31);
        assert_eq!(days_in_year_month(2019, 6), 30);
        assert_eq!(days_in_year_month(2019, 7), 31);
        assert_eq!(days_in_year_month(2019, 8), 31);
        assert_eq!(days_in_year_month(2019, 9), 30);
        assert_eq!(days_in_year_month(2019, 10), 31);
        assert_eq!(days_in_year_month(2019, 11), 30);
        assert_eq!(days_in_year_month(2019, 12), 31);

        // Leap year
        assert_eq!(days_in_year_month(2020, 1), 31);
        assert_eq!(days_in_year_month(2020, 2), 29);
        assert_eq!(days_in_year_month(2020, 3), 31);
        assert_eq!(days_in_year_month(2020, 4), 30);
        assert_eq!(days_in_year_month(2020, 5), 31);
        assert_eq!(days_in_year_month(2020, 6), 30);
        assert_eq!(days_in_year_month(2020, 7), 31);
        assert_eq!(days_in_year_month(2020, 8), 31);
        assert_eq!(days_in_year_month(2020, 9), 30);
        assert_eq!(days_in_year_month(2020, 10), 31);
        assert_eq!(days_in_year_month(2020, 11), 30);
        assert_eq!(days_in_year_month(2020, 12), 31);
    }
}
