use core::ascii;
use core::borrow::Borrow;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Display;
use core::fmt::Formatter;
use core::fmt::Write;
use core::ops::Deref;
use core::ptr;
use core::str;

/// A valid character for a node name.
///
/// Node names are restricted to alphanumeric ASCII, commas, periods, underscores, plus signs, and dashes
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Char {
    Digit0 = b'0',
    Digit1 = b'1',
    Digit2 = b'2',
    Digit3 = b'3',
    Digit4 = b'4',
    Digit5 = b'5',
    Digit6 = b'6',
    Digit7 = b'7',
    Digit8 = b'8',
    Digit9 = b'9',
    CapitalA = b'A',
    CapitalB = b'B',
    CapitalC = b'C',
    CapitalD = b'D',
    CapitalE = b'E',
    CapitalF = b'F',
    CapitalG = b'G',
    CapitalH = b'H',
    CapitalI = b'I',
    CapitalJ = b'J',
    CapitalK = b'K',
    CapitalL = b'L',
    CapitalM = b'M',
    CapitalN = b'N',
    CapitalO = b'O',
    CapitalP = b'P',
    CapitalQ = b'Q',
    CapitalR = b'R',
    CapitalS = b'S',
    CapitalT = b'T',
    CapitalU = b'U',
    CapitalV = b'V',
    CapitalW = b'W',
    CapitalX = b'X',
    CapitalY = b'Y',
    CapitalZ = b'Z',
    LowerA = b'a',
    LowerB = b'b',
    LowerC = b'c',
    LowerD = b'd',
    LowerE = b'e',
    LowerF = b'f',
    LowerG = b'g',
    LowerH = b'h',
    LowerI = b'i',
    LowerJ = b'j',
    LowerK = b'k',
    LowerL = b'l',
    LowerM = b'm',
    LowerN = b'n',
    LowerO = b'o',
    LowerP = b'p',
    LowerQ = b'q',
    LowerR = b'r',
    LowerS = b's',
    LowerT = b't',
    LowerU = b'u',
    LowerV = b'v',
    LowerW = b'w',
    LowerX = b'x',
    LowerY = b'y',
    LowerZ = b'z',
    Comma = b',',
    Period = b'.',
    Underscore = b'_',
    PlusSign = b'+',
    Dash = b'-',
}

impl Char {
    /// Returns whether or not a given `u8` byte is a valid `Char`
    const fn is_valid(byte: u8) -> bool {
        matches!(byte, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b',' | b'.' | b'_' | b'+' | b'-')
    }
}

impl From<Char> for ascii::Char {
    #[inline]
    fn from(value: Char) -> Self {
        #[expect(
            clippy::as_conversions,
            reason = "No other way to extract underlying value"
        )]
        Self::from_u8(value as u8).expect("Valid node names are always ASCII")
    }
}

/// A wrapper type for a slice of node name characters
#[repr(transparent)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct NameSlice([Char]);

impl<'chars> From<&'chars [Char]> for &'chars NameSlice {
    #[inline]
    fn from(value: &'chars [Char]) -> Self {
        #[expect(
            clippy::as_conversions,
            reason = "No other way to cast unsized pointer types"
        )]
        let pointer = ptr::from_ref(value) as *const NameSlice;
        // SAFETY: The lifetime of the new shared reference is tied to that of the old shared reference guaranteeing aliasing rules
        // and the pointer is valid because it was derived from a valid value
        unsafe { pointer.as_ref() }.expect("Pointer should be derived from a non-null reference")
    }
}

impl TryFrom<&[u8]> for &NameSlice {
    type Error = ();

    #[inline]
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        value
            .iter()
            .all(|&byte| Char::is_valid(byte))
            .then(|| {
                #[expect(
                    clippy::as_conversions,
                    reason = "No other way to cast unsized pointer types"
                )]
                let pointer = ptr::from_ref(value) as *const NameSlice;
                // SAFETY: the pointer was derived from a valid reference and the types are transmutable from one to another because the validity of the characters has been checked
                unsafe { pointer.as_ref() }
                    .expect("Pointer should be derived from a non-null reference")
            })
            .ok_or(())
    }
}

impl<'chars> From<&'chars NameSlice> for &'chars [ascii::Char] {
    #[inline]
    fn from(value: &'chars NameSlice) -> Self {
        #[expect(
            clippy::as_conversions,
            reason = "No other way to cast unsized pointer types"
        )]
        let pointer = ptr::from_ref(&value.0) as *const [ascii::Char];
        // SAFETY: The lifetime of the new shared reference is tied to that of the old shared reference guaranteeing aliasing rules
        // and the pointer is valid because it was derived from a valid value
        unsafe { pointer.as_ref() }.expect("Pointer should be derived from a non-null reference")
    }
}

impl Deref for NameSlice {
    type Target = [Char];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<NameSlice> for Box<[Char]> {
    #[inline]
    fn borrow(&self) -> &NameSlice {
        self.as_ref().into()
    }
}

impl ToOwned for NameSlice {
    type Owned = Box<[Char]>;

    fn to_owned(&self) -> Self::Owned {
        Box::from(&self.0)
    }

    fn clone_into(&self, target: &mut Self::Owned) {
        target.copy_from_slice(&self.0);
    }
}

/// Represents a node's name via borrowing
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct NameRef<'bytes> {
    /// The node-name component of the name
    node_name: &'bytes NameSlice,
    /// The unit-address component of the name
    unit_address: Option<u64>,
}

impl NameRef<'_> {
    /// Returns the node-name component of this name
    pub const fn node_name(&self) -> &NameSlice {
        self.node_name
    }

    /// Returns the unit-address component of this name, if it exists
    pub const fn unit_address(&self) -> Option<u64> {
        self.unit_address
    }
}

impl NameRef<'_> {
    /// The node-name component of a name must be 1-31 characters long
    const MAX_NODE_NAME_LENGTH: usize = 31;
}

/// Errors that can occur while parsing a slice of bytes into a `NameRef`
#[derive(Debug)]
pub enum NameRefError {
    /// A non-permitted character was in the name
    InvalidCharacters,
    /// The node-name component of a name must be 1-31 characters long
    TooLong,
}

impl<'bytes> TryFrom<&'bytes [u8]> for NameRef<'bytes> {
    type Error = NameRefError;

    #[inline]
    fn try_from(value: &'bytes [u8]) -> Result<Self, Self::Error> {
        value.split_once(|&char| char == b'@').map_or_else(
            || {
                (value.len() <= Self::MAX_NODE_NAME_LENGTH)
                    .then(|| {
                        value
                            .try_into()
                            .map_err(|()| NameRefError::InvalidCharacters)
                            .map(|node_name| Self {
                                node_name,
                                unit_address: None,
                            })
                    })
                    .unwrap_or(Err(NameRefError::TooLong))
            },
            |(node_name, unit_address)| {
                let mut address_parts = unit_address.split(|&char| char == b',');
                let address = address_parts
                    .next()
                    .expect("Split iterator should always have at least one entry");
                if address_parts.next().is_some() {
                    eprintln!(
                        "WARNING: unhandled comma in unit address: {}@{}",
                        str::from_utf8(node_name).unwrap_or("{invalid}"),
                        str::from_utf8(unit_address).unwrap_or("{invalid}"),
                    );
                }
                (node_name.len() <= Self::MAX_NODE_NAME_LENGTH)
                    .then(|| {
                        node_name
                            .try_into()
                            .ok()
                            .zip(
                                address
                                    .as_ascii()
                                    .and_then(|x| u64::from_str_radix(x.as_str(), 16).ok()),
                            )
                            .ok_or(NameRefError::InvalidCharacters)
                            .map(|(parsed_node_name, parsed_unit_address)| Self {
                                node_name: parsed_node_name,
                                unit_address: Some(parsed_unit_address),
                            })
                    })
                    .unwrap_or(Err(NameRefError::TooLong))
            },
        )
    }
}

impl Debug for Char {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        <Self as Display>::fmt(self, formatter)
    }
}

impl Display for Char {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_char(ascii::Char::from(*self).to_char())
    }
}

impl Debug for NameSlice {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        <Self as Display>::fmt(self, formatter)
    }
}

impl Display for NameSlice {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        for char in &self.0 {
            formatter.write_char(ascii::Char::from(*char).to_char())?;
        }
        Ok(())
    }
}

impl Debug for NameRef<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        <Self as Display>::fmt(self, formatter)
    }
}

impl Display for NameRef<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        if let Some(unit_address) = self.unit_address {
            write!(formatter, "{}@{}", self.node_name, unit_address)
        } else {
            write!(formatter, "{}", self.node_name)
        }
    }
}
