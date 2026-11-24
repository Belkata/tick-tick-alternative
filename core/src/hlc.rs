//! Hybrid logical clock (plan/05 P2P Sync).
//!
//! An HLC is wall-clock milliseconds, a counter for events in the same
//! millisecond, and the device id as the final tie-breaker. That gives every op
//! a unique, totally ordered stamp even when device clocks are skewed.
//!
//! The text form is fixed-width hex, so comparing strings (as SQLite does)
//! orders stamps the same way as comparing the structs.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hlc {
    /// Milliseconds since the Unix epoch (48 bits are plenty).
    pub wall: u64,
    pub counter: u32,
    pub device: u64,
}

impl Hlc {
    pub const fn zero(device: u64) -> Self {
        Self {
            wall: 0,
            counter: 0,
            device,
        }
    }

    /// The next local stamp: never behind `self`, never behind the wall clock.
    pub fn tick(self, now_ms: u64) -> Self {
        if now_ms > self.wall {
            Self {
                wall: now_ms,
                counter: 0,
                device: self.device,
            }
        } else {
            Self {
                wall: self.wall,
                counter: self.counter + 1,
                device: self.device,
            }
        }
    }

    /// Advance the local clock past a stamp received from another device, so
    /// local edits made afterwards win over what was just received.
    pub fn observe(self, remote: Hlc, now_ms: u64) -> Self {
        let wall = now_ms.max(self.wall).max(remote.wall);
        let counter = if wall == self.wall && wall == remote.wall {
            self.counter.max(remote.counter) + 1
        } else if wall == self.wall {
            self.counter + 1
        } else if wall == remote.wall {
            remote.counter + 1
        } else {
            0
        };
        Self {
            wall,
            counter,
            device: self.device,
        }
    }
}

impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:012x}-{:06x}-{:016x}",
            self.wall, self.counter, self.device
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error("malformed HLC '{0}'")]
pub struct ParseHlcError(String);

impl FromStr for Hlc {
    type Err = ParseHlcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseHlcError(s.to_string());
        let mut parts = s.split('-');
        let (Some(w), Some(c), Some(d), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(err());
        };
        if w.len() != 12 || c.len() != 6 || d.len() != 16 {
            return Err(err());
        }
        Ok(Self {
            wall: u64::from_str_radix(w, 16).map_err(|_| err())?,
            counter: u32::from_str_radix(c, 16).map_err(|_| err())?,
            device: u64::from_str_radix(d, 16).map_err(|_| err())?,
        })
    }
}

/// Serialized as its text form, so JSON op dumps stay readable and sortable.
impl serde::Serialize for Hlc {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Hlc {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn tick_is_monotonic_when_the_clock_goes_back() {
        let a = Hlc::zero(1).tick(1000);
        let b = a.tick(900); // clock stepped back
        let c = b.tick(1000);
        assert!(a < b && b < c);
        assert_eq!(b.wall, 1000);
        assert_eq!(c.counter, 2);
    }

    #[test]
    fn observe_moves_past_remote() {
        let local = Hlc::zero(1).tick(1000);
        let remote = Hlc {
            wall: 5000,
            counter: 7,
            device: 2,
        };
        let next = local.observe(remote, 1200);
        assert!(next > remote);
        assert_eq!(next.device, 1);
        assert!(next.tick(1300) > remote);
    }

    #[test]
    fn text_round_trips() {
        let h = Hlc {
            wall: 1_764_000_000_000,
            counter: 3,
            device: 0xabcdef,
        };
        assert_eq!(h.to_string().parse::<Hlc>().unwrap(), h);
        assert!("nope".parse::<Hlc>().is_err());
    }

    proptest! {
        /// String order must equal struct order, because SQLite compares the text.
        #[test]
        fn text_order_matches_struct_order(
            w1 in 0u64..(1 << 48), c1 in 0u32..(1 << 24), d1: u64,
            w2 in 0u64..(1 << 48), c2 in 0u32..(1 << 24), d2: u64,
        ) {
            let a = Hlc { wall: w1, counter: c1, device: d1 };
            let b = Hlc { wall: w2, counter: c2, device: d2 };
            prop_assert_eq!(a.cmp(&b), a.to_string().cmp(&b.to_string()));
        }
    }
}
