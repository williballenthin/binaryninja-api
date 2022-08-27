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

use crate::dwarfdebuginfo::DebugInfoBuilder;
use crate::helpers::*;

use binaryninja::{
    rc::*,
    types::{
        Enumeration, EnumerationBuilder, FunctionParameter, MemberAccess, MemberScope, Structure,
        StructureBuilder, StructureType, Type, TypeBuilder,
    },
};

use gimli::{
    constants,
    AttributeValue::{Encoding, UnitRef},
    DebuggingInformationEntry, Dwarf, Reader, Unit, UnitOffset,
};

use binaryninja::types::{NamedTypeReference, NamedTypeReferenceClass, QualifiedName};
use std::ffi::CString;

// Type tags in hello world:
//   DW_TAG_array_type
//   DW_TAG_base_type
//   DW_TAG_pointer_type
//   DW_TAG_structure_type
//   DW_TAG_typedef
//   DW_TAG_unspecified_type  // This one is done, but only for C/C++; Will not implement the generic case; Is always language specific (we just return void)
//   DW_TAG_enumeration_type
//   DW_TAG_const_type
//   DW_TAG_subroutine_type
//   DW_TAG_union_type
//   DW_TAG_class_type

//   *DW_TAG_reference_type
//   *DW_TAG_rvalue_reference_type
//   *DW_TAG_subrange_type
//   *DW_TAG_template_type_parameter
//   *DW_TAG_template_value_parameter
// * = Not yet handled
// Other tags in hello world:
//   DW_TAG_compile_unit
//   DW_TAG_namespace
//   DW_TAG_subprogram
//   DW_TAG_formal_parameter
//   DW_TAG_enumerator
//   ?DW_TAG_member
//   *DW_TAG_imported_declaration
//   *DW_TAG_imported_module
//   *DW_TAG_inheritance
//   *DW_TAG_unspecified_parameters - partially
//   *DW_TAG_variable

fn do_structure_parse<R: Reader<Offset = usize>>(
    structure_type: StructureType,
    dwarf: &Dwarf<R>,
    unit: &Unit<R>,
    entry: &DebuggingInformationEntry<R>,
    mut debug_info_builder: &mut DebugInfoBuilder<UnitOffset>,
) -> Option<Ref<Type>> {
    // bn::Types::Structure related things
    //  Steps to parsing a structure:
    //    Parse the size of the structure and create a Structure instance
    //    Recurse on the DIE's children to create all their types (any references back to the the current DIE will be unresolved NamedTypeReferences (gets resolved by the core))
    //    Populate the members of the structure, create a structure_type, and register it with the DebugInfo

    // All struct, union, and class types will have:
    //   *DW_AT_name
    //   *DW_AT_byte_size or *DW_AT_bit_size
    //   *DW_AT_declaration
    //   *DW_AT_signature
    //   *DW_AT_specification
    //   ?DW_AT_abstract_origin
    //   ?DW_AT_accessibility
    //   ?DW_AT_allocated
    //   ?DW_AT_associated
    //   ?DW_AT_data_location
    //   ?DW_AT_description
    //   ?DW_AT_start_scope
    //   ?DW_AT_visibility
    //   * = Optional

    // Structure/Class/Union _Children_ consist of:
    //  Data members:
    //   DW_AT_type
    //   *DW_AT_name
    //   *DW_AT_accessibility (default private for classes, public for everything else)
    //   *DW_AT_mutable
    //   *DW_AT_data_member_location xor *DW_AT_data_bit_offset (otherwise assume zero) <- there are some deprecations for DWARF 4
    //   *DW_AT_byte_size xor DW_AT_bit_size, iff the storage size is different than it usually would be for the given member type
    //  Function members:
    //   *DW_AT_accessibility (default private for classes, public for everything else)
    //   *DW_AT_virtuality (assume false)
    //      If true: DW_AT_vtable_elem_location
    //   *DW_AT_explicit (assume false)
    //   *DW_AT_object_pointer (assume false; for non-static member function; references the formal parameter that has "DW_AT_artificial = true" and represents "self" or "this" (language specified))
    //   *DW_AT_specification
    //   * = Optional

    // TODO : Account for DW_AT_specification
    // TODO : This should possibly be bubbled up to our parent function and generalized for all the specification/declaration things
    if let Ok(Some(_)) = entry.attr(constants::DW_AT_declaration) {
        return None;
    }

    // First things first, let's register a reference type for this struct for any children to grab while we're still building this type
    match get_name(&dwarf, &unit, &entry) {
        Some(name) => {
            println!("Add type 1");
            debug_info_builder.add_type(
                entry.offset(),
                name.clone(),
                Type::named_type(&NamedTypeReference::new(
                    NamedTypeReferenceClass::StructNamedTypeClass,
                    QualifiedName::from(name),
                )),
            );
        }
        _ => return None,
    };

    // Create structure with proper size
    let size = get_size_as_u64(&entry).unwrap_or(0);
    let mut structure_builder: StructureBuilder = StructureBuilder::new();
    structure_builder
        .set_width(size)
        .set_structure_type(structure_type);

    // Get all the children and populate
    let mut tree = unit.entries_tree(Some(entry.offset())).unwrap();
    let mut children = tree.root().unwrap().children();
    while let Ok(Some(child)) = children.next() {
        if child.entry().tag() == constants::DW_TAG_member {
            if let Ok(Some(UnitRef(child_type_offset))) =
                child.entry().attr_value(constants::DW_AT_type)
            {
                let child_type_entry = unit.entry(child_type_offset).unwrap();
                println!("  get_type : 1");
                if let Some(child_type_id) =
                    get_type(&dwarf, &unit, &child_type_entry, &mut debug_info_builder)
                {
                    if let (Some(child_name), Some((_, child_type))) = (
                        get_name(&dwarf, &unit, &child.entry()),
                        debug_info_builder.get_type(child_type_id),
                    ) {
                        // TODO : This will only work on a subset of debug data - see listed traits above
                        if let Ok(Some(raw_struct_offset)) =
                            child.entry().attr(constants::DW_AT_data_member_location)
                        {
                            let struct_offset = get_attr_as_u64(raw_struct_offset).unwrap();
                            // TODO : Verify that we shouldn't be overwriting offsets
                            structure_builder.insert(
                                child_type.as_ref(),
                                child_name,
                                struct_offset,
                                false,
                                MemberAccess::NoAccess, // TODO : Resolve actual scopes, if possible
                                MemberScope::NoScope,
                            );
                        } else if structure_type == StructureType::UnionStructureType {
                            structure_builder.append(
                                child_type.as_ref(),
                                child_name,
                                MemberAccess::NoAccess,
                                MemberScope::NoScope,
                            );
                        }
                    }
                } else if let Some(child_name) = get_name(&dwarf, &unit, &child.entry()) {
                    println!(
                        "  Couldn't parse type for member `{}` of `{:?}`",
                        child_name.to_str().unwrap(),
                        get_name(&dwarf, &unit, &entry).unwrap_or(CString::new("???").unwrap())
                    );
                } else {
                    println!("  No name and no type for member");
                }
            }
        } else if let Some(_) = {
            println!("  get_type : 2");
            get_type(&dwarf, &unit, &child.entry(), &mut debug_info_builder)
        } {
        } else if child.entry().tag() == constants::DW_TAG_subprogram {
        } else {
            println!(
                "  Missing structure child type ({:} of {:})",
                child.entry().tag(),
                entry.tag()
            );
            // Triggering on:
            //   DW_TAG_enumerator
            //   DW_TAG_enumeration_type
            //   DW_TAG_typedef
            //   DW_TAG_structure_type
            //   DW_TAG_file_type
            //   DW_TAG_union_type
            //   DW_TAG_inheritance
            //   DW_TAG_const_type
        }
    }
    // End children recursive block

    debug_info_builder.remove_type(entry.offset());

    // TODO : Figure out how to make this nicer:
    Some(Type::structure(Structure::new(&structure_builder).as_ref()))
}

// This function iterates up through the dependency references, adding all the types along the way until there are no more or stopping at the first one already tracked, then returns the UID of the type of the given DIE
// TODO : Add a fail_list of UnitOffsets that already haven't been able to be parsed as not to duplicate work
pub(crate) fn get_type<R: Reader<Offset = usize>>(
    dwarf: &Dwarf<R>,
    unit: &Unit<R>,
    entry: &DebuggingInformationEntry<R>,
    mut debug_info_builder: &mut DebugInfoBuilder<UnitOffset>,
) -> Option<UnitOffset> {
    println!(
        "Parsing: #0x{:08x}",
        match entry.offset().to_unit_section_offset(unit) {
            gimli::UnitSectionOffset::DebugInfoOffset(o) => o.0,
            gimli::UnitSectionOffset::DebugTypesOffset(o) => o.0,
        }
    );

    // If this node (and thus all its referenced nodes) has already been processed, just return the offset
    if debug_info_builder.contains_type(entry.offset()) {
        return Some(entry.offset());
    }

    // Recurse
    // TODO : Need to consider specification and abstract origin?
    let parent = match entry.attr_value(constants::DW_AT_type) {
        Ok(Some(UnitRef(parent_type_offset))) => {
            // typedefs are the devil: do not trust them
            // Typedefs should be transparent; typedefs mask the base type they refer to, not other typedefs
            if entry.tag() == constants::DW_TAG_typedef {
                let mut parent = entry.clone(); // TODO : Murder the crows?
                while let Ok(Some(UnitRef(parent_type_offset))) =
                    parent.attr_value(constants::DW_AT_type)
                {
                    parent = unit.entry(parent_type_offset).unwrap();
                }
                get_type(&dwarf, &unit, &parent, &mut debug_info_builder)
            } else {
                let entry = unit.entry(parent_type_offset).unwrap();
                get_type(&dwarf, &unit, &entry, &mut debug_info_builder)
            }
        }
        _ => None,
    };

    // If this node (and thus all its referenced nodes) has already been processed (during recursion), just return the offset
    if debug_info_builder.contains_type(entry.offset()) {
        return Some(entry.offset());
    }

    // Collect the required information to create a type and add it to the type map. Also, add the dependencies of this type to the type's typeinfo
    // Create the type, make a typeinfo for it, and add it to the debug info
    // TODO : Add this type to the type map thing
    // TODO : Add this type's dependency to the type's info
    let type_def: Option<Ref<Type>> = match entry.tag() {
        constants::DW_TAG_base_type => {
            // All base types have:
            //   DW_AT_name
            //   DW_AT_encoding (our concept of type_class)
            //   DW_AT_byte_size and/or DW_AT_bit_size
            //   *DW_AT_endianity (assumed default for arch)
            //   *DW_AT_data_bit_offset (assumed 0)
            //   *Some indication of signedness?
            //   * = Optional

            // TODO : Namespaces?
            // TODO : By spec base types need to have a name, what if it's spec non-conforming?
            let name = get_name(&dwarf, &unit, &entry).unwrap_or_else(|| {
                CString::new("TODO : 2 Put the commented out line back here instead").unwrap()
            });
            // expect("DW_TAG_base does not have name attribute");

            // TODO : Handle other size specifiers (bits, offset, high_pc?, etc)
            let size = get_size_as_usize(&entry).expect("DW_TAG_base does not have size attribute");

            match entry.attr_value(constants::DW_AT_encoding) {
                // TODO : Need more binaries to see what's going on
                Ok(Some(Encoding(encoding))) => {
                    match encoding {
                        constants::DW_ATE_address => None,
                        constants::DW_ATE_boolean => Some(Type::bool()),
                        constants::DW_ATE_complex_float => None,
                        constants::DW_ATE_float => Some(Type::named_float(size, name)),
                        constants::DW_ATE_signed => Some(Type::named_int(size, true, name)),
                        constants::DW_ATE_signed_char => Some(Type::named_int(size, true, name)),
                        constants::DW_ATE_unsigned => Some(Type::named_int(size, false, name)),
                        constants::DW_ATE_unsigned_char => Some(Type::named_int(size, false, name)),
                        constants::DW_ATE_imaginary_float => None,
                        constants::DW_ATE_packed_decimal => None,
                        constants::DW_ATE_numeric_string => None,
                        constants::DW_ATE_edited => None,
                        constants::DW_ATE_signed_fixed => None,
                        constants::DW_ATE_unsigned_fixed => None,
                        constants::DW_ATE_decimal_float => Some(Type::named_float(size, name)), // TODO : How is this different from binary floating point, ie. DW_ATE_float?
                        constants::DW_ATE_UTF => Some(Type::named_int(size, false, name)), // TODO : Verify
                        constants::DW_ATE_UCS => None,
                        constants::DW_ATE_ASCII => None, // Some sort of array?
                        constants::DW_ATE_lo_user => None,
                        constants::DW_ATE_hi_user => None,
                        _ => None, // Anything else is invalid at time of writing (gimli v0.23.0)
                    }
                }
                _ => None,
            }
        }

        constants::DW_TAG_structure_type => do_structure_parse(
            StructureType::StructStructureType,
            &dwarf,
            &unit,
            &entry,
            &mut debug_info_builder,
        ),
        constants::DW_TAG_class_type => do_structure_parse(
            StructureType::ClassStructureType,
            &dwarf,
            &unit,
            &entry,
            &mut debug_info_builder,
        ),
        constants::DW_TAG_union_type => do_structure_parse(
            StructureType::UnionStructureType,
            &dwarf,
            &unit,
            &entry,
            &mut debug_info_builder,
        ),

        // Enum
        constants::DW_TAG_enumeration_type => {
            // All base types have:
            //   DW_AT_byte_size
            //   *DW_AT_name
            //   *DW_AT_enum_class
            //   *DW_AT_type
            //   ?DW_AT_abstract_origin
            //   ?DW_AT_accessibility
            //   ?DW_AT_allocated
            //   ?DW_AT_associated
            //   ?DW_AT_bit_size
            //   ?DW_AT_bit_stride
            //   ?DW_AT_byte_stride
            //   ?DW_AT_data_location
            //   ?DW_AT_declaration
            //   ?DW_AT_description
            //   ?DW_AT_sibling
            //   ?DW_AT_signature
            //   ?DW_AT_specification
            //   ?DW_AT_start_scope
            //   ?DW_AT_visibility
            //   * = Optional

            // Children of enumeration_types are enumerators which contain:
            //  DW_AT_name
            //  DW_AT_const_value
            //  *DW_AT_description

            let mut enumeration_builder = EnumerationBuilder::new();

            let mut tree = unit.entries_tree(Some(entry.offset())).unwrap();
            let mut children = tree.root().unwrap().children();
            while let Ok(Some(child)) = children.next() {
                if child.entry().tag() == constants::DW_TAG_enumerator {
                    let name = get_name(&dwarf, &unit, &child.entry()).unwrap_or_else(|| {
                        CString::new("TODO : 3 Put the commented out line back here instead")
                            .unwrap()
                    });
                    // .expect("DW_TAG_enumeration_type does not have name attribute");
                    let value = get_attr_as_u64(
                        child
                            .entry()
                            .attr(constants::DW_AT_const_value)
                            .unwrap()
                            .unwrap(),
                    )
                    .unwrap();

                    enumeration_builder.insert(name, value);
                }
            }

            let enumeration = Enumeration::new(&enumeration_builder);

            // TODO : Get size
            Some(Type::enumeration(&enumeration, 8, false))
        }

        // Basic types
        constants::DW_TAG_typedef => {
            println!("  Typedef");
            // All base types have:
            //   DW_AT_name
            //   *DW_AT_type
            //   * = Optional

            let name = get_name(&dwarf, &unit, &entry)
                .expect("DW_TAG_typedef does not have name attribute");

            if let Some(parent_offset) = parent {
                // TODO : Remove if-let gaurd
                let parent_type = debug_info_builder.get_type(parent_offset).unwrap().1;
                Some(Type::named_type_from_type(name, parent_type.as_ref()))
            } else {
                // 5.3: "typedef represents a declaration of the type that is not also a definition"
                None
            }
        }
        constants::DW_TAG_pointer_type => {
            // All pointer types have:
            //   DW_AT_type
            //   *DW_AT_byte_size
            //   ?DW_AT_name
            //   ?DW_AT_address
            //   ?DW_AT_allocated
            //   ?DW_AT_associated
            //   ?DW_AT_data_location
            //   * = Optional

            // TODO : We assume the parent has a name?  Might we need to resolve it deeper?
            // TODO : Investigate node to see if we need to traverse deeper

            // TODO : Remove this if-let thing, just to get Jordan's binary to work

            if let Some(pointer_size) = get_size_as_usize(&entry) {
                if let Some(parent_offset) = parent {
                    let parent_type = debug_info_builder.get_type(parent_offset).unwrap().1;
                    match get_name(&dwarf, &unit, &unit.entry(parent_offset).unwrap()) {
                        Some(name) => Some(Type::pointer_of_width(
                            Type::named_type_from_type(name, parent_type.as_ref()).as_ref(),
                            // Not sure about the named_type id stuff
                            // Type::named_type(&NamedTypeReference::new(
                            //     NamedTypeReferenceClass::UnknownNamedTypeClass,
                            //     "",
                            //     QualifiedName::from(name),
                            // ))
                            // .as_ref(),
                            pointer_size,
                            false,
                            false,
                            None,
                        )),
                        _ => None,
                    }
                } else {
                    Some(Type::pointer_of_width(
                        Type::void().as_ref(),
                        pointer_size,
                        false,
                        false,
                        None,
                    ))
                }
            } else {
                None
            }
        }
        constants::DW_TAG_array_type => {
            // All array types have:
            //    DW_AT_type
            //   *DW_AT_name
            //   *DW_AT_ordering
            //   *DW_AT_byte_stride or DW_AT_bit_stride
            //   *DW_AT_byte_size or DW_AT_bit_size
            //   *DW_AT_allocated
            //   *DW_AT_associated and
            //   *DW_AT_data_location
            //   * = Optional
            //   For multidimensional arrays, DW_TAG_subrange_type or DW_TAG_enumeration_type

            // TODO : How to do the name, if it has one?
            // TODO : size
            if let Some(parent_offset) = parent {
                let parent_type = debug_info_builder.get_type(parent_offset).unwrap().1;
                Some(Type::array(parent_type.as_ref(), 0))
            } else {
                None
            }
        }
        constants::DW_TAG_string_type => None,

        // Strange Types
        constants::DW_TAG_unspecified_type => Some(Type::void()),
        constants::DW_TAG_subroutine_type => {
            // All subroutine types have:
            //   *DW_AT_name
            //   *DW_AT_type (if not provided, void)
            //   *DW_AT_prototyped
            //   ?DW_AT_abstract_origin
            //   ?DW_AT_accessibility
            //   ?DW_AT_address_class
            //   ?DW_AT_allocated
            //   ?DW_AT_associated
            //   ?DW_AT_data_location
            //   ?DW_AT_declaration
            //   ?DW_AT_description
            //   ?DW_AT_sibling
            //   ?DW_AT_start_scope
            //   ?DW_AT_visibility
            //   * = Optional

            // May have children, including DW_TAG_formal_parameters, which all have:
            //   *DW_AT_type
            //   * = Optional
            // or is otherwise DW_TAG_unspecified_parameters

            let return_type = match parent {
                Some(parent_offset) => debug_info_builder
                    .get_type(parent_offset)
                    .expect("Subroutine return type was not processed")
                    .1
                    .clone(),
                None => Type::void(),
            };

            let mut parameters: Vec<FunctionParameter<CString>> = vec![];
            let mut variable_arguments = false;

            // Get all the children and populate
            // TODO : Handle other attributes?
            let mut tree = unit.entries_tree(Some(entry.offset())).unwrap();
            let mut children = tree.root().unwrap().children();
            while let Ok(Some(child)) = children.next() {
                if child.entry().tag() == constants::DW_TAG_formal_parameter {
                    if let (Some(child_uid), Some(name)) = {
                        println!("  get_type : 5");
                        (
                            get_type(&dwarf, &unit, &child.entry(), &mut debug_info_builder),
                            get_name(&dwarf, &unit, &child.entry()),
                        )
                    } {
                        let child_type = debug_info_builder.get_type(child_uid).unwrap().1;
                        parameters.push(FunctionParameter::new(
                            child_type,
                            CString::new(name).unwrap(),
                            None,
                        )); // TODO : I think I can remove this call to new
                    } else {
                        println!("Failed to parse child type");
                    }
                } else if child.entry().tag() == constants::DW_TAG_unspecified_parameters {
                    variable_arguments = true;
                }
            }

            Some(Type::function(
                return_type.as_ref(),
                &parameters,
                variable_arguments,
            ))
        }

        // Unusual Types
        constants::DW_TAG_ptr_to_member_type => None,
        constants::DW_TAG_set_type => None,
        constants::DW_TAG_subrange_type => None,
        constants::DW_TAG_file_type => None,
        constants::DW_TAG_thrown_type => None,
        constants::DW_TAG_interface_type => None,

        // Weird types
        constants::DW_TAG_reference_type => None, // This is the l-value for the complimentary r-value following in the if-else chain
        constants::DW_TAG_rvalue_reference_type => None,
        constants::DW_TAG_restrict_type => None,
        constants::DW_TAG_shared_type => None,
        constants::DW_TAG_volatile_type => None,
        constants::DW_TAG_packed_type => None,
        constants::DW_TAG_const_type => {
            // All const types have:
            //   ?DW_AT_allocated
            //   ?DW_AT_associated
            //   ?DW_AT_data_location
            //   ?DW_AT_name
            //   ?DW_AT_sibling
            //   ?DW_AT_type

            if let Some(parent_offset) = parent {
                let parent_type = debug_info_builder.get_type(parent_offset).unwrap().1;
                Some((*parent_type).to_builder().set_const(true).finalize())
            } else {
                Some(TypeBuilder::void().set_const(true).finalize())
            }
        }

        // Pass-through tags
        constants::DW_TAG_formal_parameter | constants::DW_TAG_subprogram => {
            if let Some(parent_offset) = parent {
                if let Some((_, result_type)) = debug_info_builder.get_type(parent_offset) {
                    Some(result_type)
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    };

    println!(
        "Finishing up with: #0x{:08x}",
        match entry.offset().to_unit_section_offset(unit) {
            gimli::UnitSectionOffset::DebugInfoOffset(o) => o.0,
            gimli::UnitSectionOffset::DebugTypesOffset(o) => o.0,
        }
    );

    // Wrap our resultant type in a TypeInfo so that the internal DebugInfo class can manage it
    // TODO : Figure out what to do with the name field
    if let Some(type_def) = type_def {
        println!("Add type 2");
        debug_info_builder.add_type(
            entry.offset(),
            get_name(&dwarf, &unit, &entry).unwrap_or_else(|| CString::new("").unwrap()), // Something smarter than ::new("")?
            type_def,
        );
        Some(entry.offset())
    } else {
        None
    }
}
