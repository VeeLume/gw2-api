//! Currency (Coin) type with gold/silver/copper display.
//!
//! The GW2 API represents currency as a single integer in copper.
//! This type provides ergonomic access to gold, silver, and copper components.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Sub};

/// Currency value stored as copper, with gold/silver/copper accessors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Coin(pub i64);

impl Coin {
    pub const ZERO: Self = Self(0);

    /// Create from individual components.
    pub fn from_parts(gold: i64, silver: i64, copper: i64) -> Self {
        Self(gold * 10_000 + silver * 100 + copper)
    }

    /// Total value in copper.
    pub fn copper_total(&self) -> i64 {
        self.0
    }

    /// Gold component.
    pub fn gold(&self) -> i64 {
        self.0.abs() / 10_000
    }

    /// Silver component (0–99).
    pub fn silver(&self) -> i64 {
        (self.0.abs() % 10_000) / 100
    }

    /// Copper component (0–99).
    pub fn copper(&self) -> i64 {
        self.0.abs() % 100
    }

    /// Whether this is a negative (loss) value.
    pub fn is_negative(&self) -> bool {
        self.0 < 0
    }
}

pub fn coin_from_u32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Coin, D::Error> {
    let v = u32::deserialize(d)?;
    Ok(Coin(v as i64))
}

impl fmt::Display for Coin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.is_negative() { "-" } else { "" };
        let g = self.gold();
        let s = self.silver();
        let c = self.copper();

        if g > 0 {
            write!(f, "{sign}{g}g {s:02}s {c:02}c")
        } else if s > 0 {
            write!(f, "{sign}{s}s {c:02}c")
        } else {
            write!(f, "{sign}{c}c")
        }
    }
}

impl Add for Coin {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Coin {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl From<i64> for Coin {
    fn from(copper: i64) -> Self {
        Self(copper)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_components() {
        let coin = Coin(1_23_45); // 1g 23s 45c
        assert_eq!(coin.gold(), 1);
        assert_eq!(coin.silver(), 23);
        assert_eq!(coin.copper(), 45);
    }

    #[test]
    fn test_display() {
        assert_eq!(Coin(12345).to_string(), "1g 23s 45c");
        assert_eq!(Coin(2345).to_string(), "23s 45c");
        assert_eq!(Coin(45).to_string(), "45c");
        assert_eq!(Coin(0).to_string(), "0c");
    }

    #[test]
    fn test_from_parts() {
        let coin = Coin::from_parts(10, 50, 99);
        assert_eq!(coin.copper_total(), 105_099);
        assert_eq!(coin.gold(), 10);
        assert_eq!(coin.silver(), 50);
        assert_eq!(coin.copper(), 99);
    }

    #[test]
    fn test_arithmetic() {
        let a = Coin(10_000);
        let b = Coin(5_000);
        assert_eq!((a + b).copper_total(), 15_000);
        assert_eq!((a - b).copper_total(), 5_000);
    }
}
