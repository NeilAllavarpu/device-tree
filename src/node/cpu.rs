//! A CPU node

use crate::{
    map::Map,
    parse::U32ByteSlice,
    property::{to_c_str, EnableType},
};
use alloc::rc::Rc;
use core::ffi::CStr;

use super::{cache_desc, Cache, HarvardCache, HigherLevelCache, Node, PropertyKeys, RawNode};

#[derive(Debug)]
pub struct CpuNode<'a> {
    pub node: Node<'a>,
    enable_method: Option<EnableType>,
    l1_cache: Cache,
    next_cache: Option<Rc<HigherLevelCache<'a>>>,
    properties: Map<&'a CStr, U32ByteSlice<'a>>,
}

impl<'a> CpuNode<'a> {
    pub(super) fn new<'b>(
        mut value: RawNode<'a>,
        base: &'b Map<&'a CStr, U32ByteSlice<'a>>,
        cache_handles: &'b Map<u32, Rc<HigherLevelCache<'a>>>,
        address_cells: u32,
    ) -> Result<Self, ()> {
        value.properties.merge_preserve(base);

        if let Some(dtype) = value.properties.remove(&PropertyKeys::DEVICE_TYPE) {
            assert_eq!(<&CStr>::try_from(dtype), Ok(to_c_str(b"cpu\0")));
        }

        let enable_method =
            if let Some(method) = value.properties.remove(&PropertyKeys::ENABLE_METHOD) {
                match EnableType::try_from(<&[u8]>::from(method)) {
                    Ok(EnableType::SpinTable(_)) => {
                        let addr = value
                            .properties
                            .remove(&PropertyKeys::CPU_RELEASE_ADDR)
                            .unwrap();
                        // let parser = ByteParser::new(u8_to_u32_slice(addr).unwrap());
                        Some(EnableType::SpinTable(addr.try_into().unwrap()))
                    }
                    Ok(vendor) => Some(vendor),
                    Err(_) => todo!(),
                }
            } else {
                None
            };

        let cache = if value
            .properties
            .remove(&PropertyKeys::CACHE_UNIFIED)
            .is_some()
        {
            Cache::Unified(cache_desc(&mut value.properties, ""))
        } else {
            Cache::Harvard(HarvardCache {
                icache: cache_desc(&mut value.properties, "i-"),
                dcache: cache_desc(&mut value.properties, "d-"),
            })
        };

        let next_cache = value
            .properties
            .remove(&PropertyKeys::NEXT_LEVEL_CACHE)
            .and_then(|phandle| cache_handles.get(&u32::try_from(phandle).unwrap()))
            .map(Rc::clone);

        let node = Node::new(value, address_cells, 0);
        Ok(Self {
            node,
            enable_method,
            l1_cache: cache,
            next_cache,
            properties: Map::new(),
        })
    }
}
