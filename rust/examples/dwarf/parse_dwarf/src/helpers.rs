// Copyright 2021-2022 Vector 35 Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use binaryninja::binaryninjacore_sys::*;
use binaryninja::binaryview::{BinaryView, BinaryViewBase, BinaryViewExt};
use binaryninja::databuffer::DataBuffer;
use binaryninja::Endianness; // TODO : Kill it with fire

use gimli::{
    constants, Attribute, AttributeValue::UnitRef, DebuggingInformationEntry, Dwarf, Endianity,
    Error, Reader, RunTimeEndian, SectionId, Unit, UnitOffset,
};

use std::ffi::CString;

use dwarfreader::DWARFReader;

//////////////////////
// Dwarf Validation

pub(crate) fn is_non_dwo_dwarf(view: &BinaryView) -> bool {
    view.section_by_name(".debug_info").is_ok()
}

pub(crate) fn is_dwo_dwarf(view: &BinaryView) -> bool {
    view.section_by_name(".debug_info.dwo").is_ok()
}

pub(crate) fn is_parent_non_dwo_dwarf(view: &BinaryView) -> bool {
    if let Ok(parent_view) = view.parent_view() {
        parent_view.section_by_name(".debug_info").is_ok()
    } else {
        false
    }
}

pub(crate) fn is_parent_dwo_dwarf(view: &BinaryView) -> bool {
    if let Ok(parent_view) = view.parent_view() {
        parent_view.section_by_name(".debug_info.dwo").is_ok()
    } else {
        false
    }
}

/////////////////////
// Reader Wrappers

pub(crate) fn get_endian(view: &BinaryView) -> RunTimeEndian {
    match view.default_endianness() {
        Endianness::LittleEndian => RunTimeEndian::Little,
        Endianness::BigEndian => RunTimeEndian::Big,
    }
}

pub(crate) fn create_section_reader<'a, Endian: 'a + Endianity>(
    view: &'a BinaryView,
    endian: Endian,
    dwo_file: bool,
) -> Box<dyn Fn(SectionId) -> Result<DWARFReader<Endian>, Error> + 'a> {
    Box::new(move |section_id: SectionId| {
        let section_name;
        if dwo_file && section_id.dwo_name().is_some() {
            section_name = section_id.dwo_name().unwrap();
        } else if dwo_file {
            return Ok(DWARFReader::new(vec![], endian));
        } else {
            section_name = section_id.name();
        }

        if let Ok(section) = view.section_by_name(section_name) {
            // TODO : This is kinda broke....should add rust wrappers for some of this
            if let Some(symbol) = view
                .symbols()
                .iter()
                .find(|symbol| symbol.full_name().as_str() == "__elf_section_headers")
            {
                if let Some(data_var) = view
                    .data_variables()
                    .iter()
                    .find(|var| var.address == symbol.address())
                {
                    // TODO : This should eventually be wrapped by some DataView sorta thingy thing, like how python does it
                    let data_type = data_var.type_with_confidence().contents;
                    let data = view.read_vec(data_var.address, data_type.width() as usize);
                    let element_type = data_type.element_type().unwrap().contents;

                    // TODO : broke af?
                    if let Some(current_section_header) = data
                        .chunks(element_type.width() as usize)
                        .find(|section_header| {
                            endian.read_u64(&section_header[24..32]) == section.start()
                        })
                    {
                        if (endian.read_u64(&current_section_header[8..16]) & 2048) != 0 {
                            // Get section, trim header, decompress, return
                            let offset = section.start() + 24; // TODO : Super broke AF
                            let len = section.len() - 24;

                            if let Ok(buffer) = view.read_buffer(offset, len as usize) {
                                // Incredibly broke as fuck
                                use std::ptr;
                                let transform_name =
                                    CString::new("Zlib").unwrap().into_bytes_with_nul();
                                let transform = unsafe {
                                    BNGetTransformByName(transform_name.as_ptr() as *mut _)
                                };

                                // Omega broke
                                let raw_buf: *mut BNDataBuffer =
                                    unsafe { BNCreateDataBuffer(ptr::null_mut(), 0) };
                                if unsafe {
                                    BNDecode(
                                        transform,
                                        std::mem::transmute(buffer),
                                        raw_buf,
                                        ptr::null_mut(),
                                        0,
                                    )
                                } {
                                    let output_buffer: DataBuffer =
                                        unsafe { std::mem::transmute(raw_buf) };

                                    return Ok(DWARFReader::new(
                                        output_buffer.get_data().into(),
                                        endian,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            let offset = section.start();
            let len = section.len();
            if len == 0 {
                return Ok(DWARFReader::new(vec![], endian));
            }
            let reader = DWARFReader::new(view.read_vec(offset, len as usize), endian);
            return Ok(reader);
        } else {
            return Ok(DWARFReader::new(vec![], endian));
        }
    })
}

////////////////////////////////////
// DIE attr convenience functions

// TODO : This only gets one kind of base entry (for...functions?), we should check for overlap and whatnot to parse specific types of base entries
pub(crate) fn get_base_entry<R: Reader>(
    unit: &Unit<R>,
    entry: &DebuggingInformationEntry<R>,
) -> UnitOffset<<R as Reader>::Offset> {
    if let Ok(Some(UnitRef(offset))) = entry.attr_value(constants::DW_AT_specification) {
        let entry = unit.entry(offset).unwrap();
        get_base_entry(unit, &entry)
    } else if let Ok(Some(UnitRef(offset))) = entry.attr_value(constants::DW_AT_abstract_origin) {
        let entry = unit.entry(offset).unwrap();
        get_base_entry(unit, &entry)
    } else {
        entry.offset()
    }
}

// Get name from DIE, or referenced dependencies
// TODO : Ensure this encapsulates all the linkable nodes?
pub(crate) fn get_name<R: Reader>(
    dwarf: &Dwarf<R>,
    unit: &Unit<R>,
    entry: &DebuggingInformationEntry<R>,
) -> Option<CString> {
    if let Ok(Some(attr_val)) = entry.attr_value(constants::DW_AT_name) {
        if let Ok(attr_string) = dwarf.attr_string(&unit, attr_val) {
            if let Ok(attr_string) = attr_string.to_string() {
                Some(CString::new(attr_string.to_string()).unwrap())
            } else {
                None
            }
        } else {
            None
        }
    } else if let Ok(Some(UnitRef(offset))) = entry.attr_value(constants::DW_AT_specification) {
        let entry = unit.entry(offset).unwrap();
        get_name(dwarf, unit, &entry)
    } else if let Ok(Some(UnitRef(offset))) = entry.attr_value(constants::DW_AT_abstract_origin) {
        let entry = unit.entry(offset).unwrap();
        get_name(dwarf, unit, &entry)
    } else {
        None
    }
}

// Get raw name from DIE, or referenced dependencies
pub(crate) fn get_raw_name<R: Reader>(
    dwarf: &Dwarf<R>,
    unit: &Unit<R>,
    entry: &DebuggingInformationEntry<R>,
) -> Option<CString> {
    if let Ok(Some(attr_val)) = entry.attr_value(constants::DW_AT_linkage_name) {
        if let Ok(attr_string) = dwarf.attr_string(&unit, attr_val) {
            if let Ok(attr_string) = attr_string.to_string() {
                Some(CString::new(attr_string.to_string()).unwrap())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

// Construct a fully-qualified-name from a list of namespace qualifiers
pub(crate) fn recover_full_name<'a>(
    short_name: &Option<CString>,
    namespace_qualifiers: &Vec<(isize, CString)>,
) -> Option<CString> {
    // The DIE does not contain any namespace information, so we track the namespaces and build the symbol ourselves
    if let Some(function_name) = short_name {
        let mut full_name_builder = "".to_string();
        for (_, namespace) in namespace_qualifiers {
            full_name_builder = format!("{}{}::", full_name_builder, namespace.to_string_lossy());
        }
        Some(
            CString::new(format!(
                "{}{}",
                full_name_builder,
                function_name.to_string_lossy()
            ))
            .unwrap(),
        )
    } else {
        None
    }
}

// Get the size of an object as a usize
pub(crate) fn get_size_as_usize<R: Reader>(entry: &DebuggingInformationEntry<R>) -> Option<usize> {
    if let Ok(Some(attr)) = entry.attr(constants::DW_AT_byte_size) {
        get_attr_as_usize(attr)
    } else if let Ok(Some(attr)) = entry.attr(constants::DW_AT_bit_size) {
        match get_attr_as_usize(attr) {
            Some(attr_value) => Some(attr_value / 8),
            _ => None,
        }
    } else {
        None
    }
}

// Get the size of an object as a u64
pub(crate) fn get_size_as_u64<R: Reader>(entry: &DebuggingInformationEntry<R>) -> Option<u64> {
    if let Ok(Some(attr)) = entry.attr(constants::DW_AT_byte_size) {
        get_attr_as_u64(attr)
    } else if let Ok(Some(attr)) = entry.attr(constants::DW_AT_bit_size) {
        match get_attr_as_u64(attr) {
            Some(attr_value) => Some(attr_value / 8),
            _ => None,
        }
    } else {
        None
    }
}

// Get the start address of a function
pub(crate) fn get_start_address<R: Reader>(
    dwarf: &Dwarf<R>,
    unit: &Unit<R>,
    entry: &DebuggingInformationEntry<R>,
) -> Option<u64> {
    // TODO : Need to cover more address DIE address representations:
    //   DW_AT_ranges
    if let Ok(Some(attr_val)) = entry.attr_value(constants::DW_AT_low_pc) {
        match dwarf.attr_address(&unit, attr_val) {
            Ok(Some(val)) => Some(val),
            _ => None,
        }
    } else if let Ok(Some(attr_val)) = entry.attr_value(constants::DW_AT_entry_pc) {
        match dwarf.attr_address(&unit, attr_val) {
            Ok(Some(val)) => Some(val),
            _ => None,
        }
    } else {
        None
    }
}

// Get an attribute value as a u64 if it can be coerced
pub(crate) fn get_attr_as_u64<R: Reader>(attr: Attribute<R>) -> Option<u64> {
    if let Some(value) = attr.u8_value() {
        Some(value.into())
    } else if let Some(value) = attr.u16_value() {
        Some(value.into())
    } else if let Some(value) = attr.udata_value() {
        Some(value.into())
    } else if let Some(value) = attr.sdata_value() {
        Some(value as u64)
    } else {
        None
    }
}

// Get an attribute value as a usize if it can be coerced
pub(crate) fn get_attr_as_usize<R: Reader>(attr: Attribute<R>) -> Option<usize> {
    if let Some(value) = attr.u8_value() {
        Some(value.into())
    } else if let Some(value) = attr.u16_value() {
        Some(value.into())
    } else if let Some(value) = attr.udata_value() {
        Some(value as usize)
    } else if let Some(value) = attr.sdata_value() {
        Some(value as usize)
    } else {
        None
    }
}
