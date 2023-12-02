//! Types for describing caches
//!
//! The device tree provides information as caches both as a part of CPU nodes (for L1 caches) or as independent nodes (for higher caches)

use alloc::rc::Rc;

use super::{device, ChildMap, Node, PropertyMap, RawNode, RawNodeError};
use crate::{map::Map, node::PropertyKeys, parse::U32ByteSlice};
use core::{ffi::CStr, num::NonZeroU32};

// TODO: Are these not actually required for a device tree to fully implement?
/// The description of a cache node
#[derive(Debug)]
pub struct Description {
    /// Specifies the size in bytes of a cache.
    size: Option<NonZeroU32>,
    /// Specifies the number of associativity sets in a cache
    sets: Option<NonZeroU32>,
    /// Specifies the block size in bytes of a cache.
    block_size: Option<NonZeroU32>,
    /// Specifies the line size in bytes of a cache, if different than the cache block size
    line_size: Option<NonZeroU32>,
}

/// Extracts a cache description from properties, where the cache-specific keys are prefixed with the given prefix
macro_rules! cache_description {
    ($properties:expr, $prefix:expr) => {{
        Description::from_prefix(
            $properties,
            &CStr::from_bytes_with_nul(concat_bytes!($prefix, b"cache-size\0").as_slice()).unwrap(),
            &CStr::from_bytes_with_nul(concat_bytes!($prefix, b"cache-sets\0").as_slice()).unwrap(),
            &CStr::from_bytes_with_nul(concat_bytes!($prefix, b"cache-block-size\0").as_slice())
                .unwrap(),
            &CStr::from_bytes_with_nul(concat_bytes!($prefix, b"cache-line-size\0").as_slice())
                .unwrap(),
            // concat_bytes!($prefix, b"cache-sets\0"),
            // concat_bytes!($prefix, b"cache-block-size\0"),
            // concat_bytes!($prefix, b"cache-line-size\0"),
        )
    }};
}

impl Description {
    /// Extracts a cache description from properties, using the provided keys to look up properties
    fn from_prefix<'node, 'keys>(
        properties: &mut Map<&'node CStr, U32ByteSlice<'_>>,
        size_key: &'keys CStr,
        sets_key: &'keys CStr,
        block_size_key: &'keys CStr,
        line_size_key: &'keys CStr,
    ) -> Self
    where
        'keys: 'node,
    {
        // let size_key =
        // CStr::from_bytes_with_nul(format!("{PREFIX}cache-size\0",).as_bytes()).unwrap();
        Self {
            size: properties
                .remove(&size_key)
                .and_then(|value| value.try_into().ok())
                .and_then(NonZeroU32::new),
            sets: properties
                .remove(
                    sets_key, // &CStr::from_bytes_until_nul(
                             //     format!("{}cache-sets\0", PREFIX).leak().as_bytes(),
                             // )
                             // .unwrap(),
                )
                .and_then(|value| value.try_into().ok())
                .and_then(NonZeroU32::new),
            block_size: properties
                .remove(
                    block_size_key, // &CStr::from_bytes_until_nul(
                                    //     format!("{}cache-block-size\0", PREFIX).leak().as_bytes(),
                                    // )
                                    // .unwrap(),
                )
                .and_then(|value| value.try_into().ok())
                .and_then(NonZeroU32::new),
            line_size: properties
                .remove(
                    line_size_key, // &CStr::from_bytes_until_nul(
                                   //     format!("{}cache-line-size\0", PREFIX).leak().as_bytes(),
                                   // )
                                   // .unwrap(),
                )
                .and_then(|value| value.try_into().ok())
                .and_then(NonZeroU32::new),
        }
    }

    #[must_use]
    #[inline]
    pub const fn size(&self) -> Option<NonZeroU32> {
        self.size
    }

    #[must_use]
    #[inline]
    pub const fn sets(&self) -> Option<NonZeroU32> {
        self.sets
    }

    #[must_use]
    #[inline]
    pub const fn block_size(&self) -> Option<NonZeroU32> {
        self.block_size
    }

    #[must_use]
    #[inline]
    pub const fn line_size(&self) -> Option<NonZeroU32> {
        self.line_size
    }
}

/// Processors and systems may implement additional levels of cache hierarchy. For example, second-level (L2) or third-level (L3) caches.
/// These caches can potentially be tightly integrated to the CPU or possibly shared between multiple CPUs.
/// A device node with a compatible value of "cache" describes these types of caches.
/// The cache node shall define a phandle property, and all cpu nodes or cache nodes that are associated with or share the cache each shall contain a next-level-cache property that specifies the phandle to the cache node.
/// A cache node may be represented under a CPU node or any other appropriate location in the devicetree.
#[derive(Debug)]
pub struct HigherLevel<'node> {
    /// The description of the cache itself
    cache: Description,
    /// Specifies the level in the cache hierarchy. For example, a level 2 cache has a value of 2.
    level: u32,
    /// Children of this node
    children: ChildMap<'node>,
    /// Other miscellaneous properties
    properties: PropertyMap<'node>,
}

/// Errors from parsing a node into a `HigherLevel` Cache
#[non_exhaustive]
#[derive(Debug)]
pub enum HigherLevelError {
    /// The compatible field of the node is either missing or not equal to `"cache"`
    BadType,
    /// The phandle of the cache is either malformed or missing
    PHandle,
    /// The level of the cache is either missing or malformed
    Level,
    /// Error parsing the cells of this node, if present
    Cells,
    /// Error parsing a child node
    Child(device::Error),
}

impl<'node> HigherLevel<'node> {
    /// Creates a new higher-level cache from the given device tree node
    pub(super) fn new(
        mut value: RawNode<'node>,
        phandles: &mut Map<u32, Rc<device::Node<'node>>>,
    ) -> Result<(u32, Self), HigherLevelError> {
        if !value
            .properties
            .remove(&PropertyKeys::COMPATIBLE)
            .and_then(|x| <&CStr>::try_from(x).ok())
            .is_some_and(|y| y.to_bytes() == b"cache")
        {
            return Err(HigherLevelError::BadType);
        }

        let phandle = value
            .properties
            .remove(&PropertyKeys::PHANDLE)
            .and_then(|x| x.try_into().ok())
            .ok_or(HigherLevelError::PHandle)?;

        let level = value
            .properties
            .remove(&PropertyKeys::CACHE_LEVEL)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(HigherLevelError::Level)?;

        let cache = cache_description!(&mut value.properties, b"");

        let (properties, children) = value.into_components(phandles, None);
        let children = match children {
            Ok(children) => children,
            Err(RawNodeError::Cells) => return Err(HigherLevelError::Cells),
            Err(RawNodeError::Child(child)) => return Err(HigherLevelError::Child(child)),
        };
        Ok((
            phandle,
            Self {
                cache,
                level,
                children,
                properties,
            },
        ))
    }

    #[inline]
    #[must_use]
    pub const fn cache(&self) -> &Description {
        &self.cache
    }

    #[inline]
    #[must_use]
    pub const fn level(&self) -> u32 {
        self.level
    }
}

impl<'node> Node<'node> for HigherLevel<'node> {
    #[inline]
    fn properties(&self) -> &PropertyMap {
        &self.properties
    }

    #[inline]
    fn children(&self) -> &ChildMap<'node> {
        &self.children
    }
}

/// A Harvard Cache has separate caches for data and instructions
#[derive(Debug)]
pub struct Harvard {
    /// The instruction cache description
    icache: Description,
    /// The data cache description
    dcache: Description,
}

impl Harvard {
    /// Returns the instruction cache description of this Harvard cache
    #[must_use]
    #[inline]
    pub const fn dcache(&self) -> &Description {
        &self.dcache
    }

    /// Returns the data cache description of this Harvard cache
    #[must_use]
    #[inline]
    pub const fn icache(&self) -> &Description {
        &self.icache
    }
}

/// The L1 cache residing in a CPU
#[derive(Debug)]
#[expect(
    clippy::exhaustive_enums,
    reason = "These are the only possible variants as specified by the Device Tree spec"
)]
pub enum L1 {
    /// The cache is unified for both data and instructions
    Unified(Description),
    /// The cache is separated between data L1 and instruction L1
    Harvard(Harvard),
}

impl L1 {
    /// Extracts an L1 cache description from the properties of a CPU node
    pub(crate) fn extract_from(properties: &mut Map<&CStr, U32ByteSlice>) -> Self {
        if properties.remove(&PropertyKeys::CACHE_UNIFIED).is_some() {
            Self::Unified(cache_description!(properties, b""))
        } else {
            Self::Harvard(Harvard {
                icache: cache_description!(properties, b"i-"),
                dcache: cache_description!(properties, b"d-"),
            })
        }
    }
}
