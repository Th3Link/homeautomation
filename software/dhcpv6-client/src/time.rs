//! The client's notion of time: milliseconds on the caller's clock.

/// A point in time, in milliseconds since an epoch of the caller's choice
/// (boot, typically). Only differences matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(u64);

impl Instant {
    /// Later than any other time: the end of an infinite lifetime.
    pub const NEVER: Self = Self(u64::MAX);

    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    pub const fn from_secs(secs: u64) -> Self {
        Self(secs.saturating_mul(1000))
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }

    /// `self` plus `millis`; [`Self::NEVER`] stays never.
    pub(crate) const fn plus_millis(self, millis: u64) -> Self {
        Self(self.0.saturating_add(millis))
    }

    /// `self` plus a DHCPv6 lifetime in seconds, where `0xffffffff` means
    /// infinity (RFC 8415 § 7.7).
    pub(crate) const fn plus_lifetime(self, secs: u32) -> Self {
        if secs == u32::MAX {
            Self::NEVER
        } else {
            self.plus_millis(secs as u64 * 1000)
        }
    }

    /// Milliseconds from `earlier` to `self`, 0 if `earlier` is later.
    pub(crate) const fn millis_since(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infinite_lifetime_never_ends() {
        assert_eq!(
            Instant::from_secs(5).plus_lifetime(u32::MAX),
            Instant::NEVER
        );
        assert_eq!(Instant::NEVER.plus_millis(1), Instant::NEVER);
        assert_eq!(
            Instant::from_secs(5).plus_lifetime(10),
            Instant::from_secs(15)
        );
    }

    #[test]
    fn millis_since_saturates() {
        assert_eq!(
            Instant::from_millis(5).millis_since(Instant::from_millis(7)),
            0
        );
        assert_eq!(
            Instant::from_millis(7).millis_since(Instant::from_millis(5)),
            2
        );
    }
}
