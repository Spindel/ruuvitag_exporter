#![forbid(unsafe_code)]
/// License: BSD 2-clause
/// Taken from:
///    <https://docs.rs/bluer/0.15.0/src/bluer/lib.rs.html#751>
/// Bluetooth address.
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;
use std::str::FromStr;

/// The serialized representation is a string in colon-hexadecimal notation.
#[derive(Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Address(pub [u8; 6]);

impl Address {
    /// Creates a new Bluetooth address with the specified value.
    pub const fn new(addr: [u8; 6]) -> Self {
        Self(addr)
    }

    /// Any Bluetooth address.
    ///
    /// Corresponds to `00:00:00:00:00:00`.
    pub const fn any() -> Self {
        Self([0; 6])
    }
}

impl Deref for Address {
    type Target = [u8; 6];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Address {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self}")
    }
}

/*
impl From<sys::bdaddr_t> for Address {
    fn from(mut addr: sys::bdaddr_t) -> Self {
        addr.b.reverse();
        Self(addr.b)
    }
}*/
/*
impl From<Address> for sys::bdaddr_t {
    fn from(mut addr: Address) -> Self {
        addr.0.reverse();
        sys::bdaddr_t { b: addr.0 }
    }
}*/

/// Invalid Bluetooth address error.
#[derive(Debug, Clone)]
// #[derive(serde::Serialize, serde::Deserialize)]
pub struct InvalidAddress(pub String);

impl fmt::Display for InvalidAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid Bluetooth address: {}", &self.0)
    }
}

impl std::error::Error for InvalidAddress {}

impl FromStr for Address {
    type Err = InvalidAddress;
    fn from_str(s: &str) -> std::result::Result<Self, InvalidAddress> {
        let fields = s
            .split(':')
            .map(|s| u8::from_str_radix(s, 16).map_err(|_| InvalidAddress(s.to_string())))
            .collect::<std::result::Result<Vec<_>, InvalidAddress>>()?;
        Ok(Self(
            fields
                .try_into()
                .map_err(|_| InvalidAddress(s.to_string()))?,
        ))
    }
}

impl From<[u8; 6]> for Address {
    fn from(addr: [u8; 6]) -> Self {
        Self(addr)
    }
}

impl From<Address> for [u8; 6] {
    fn from(addr: Address) -> Self {
        addr.0
    }
}

/*
impl serde::Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(|err| D::Error::custom(&err))
    }
}
*/
