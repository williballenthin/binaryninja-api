use fallible_iterator::FallibleIterator;
use gimli::{
    Dwarf, Endianity, Reader, RunTimeEndian, Section, SectionId, UnitHeader, UnitOffset,
    UnitSectionOffset, UnitType, UnwindSection,
};
use regex::bytes::Regex;
use std::{
    collections::HashMap,
    env,
    ffi::CString,
    fmt::{self, Debug},
    io,
    io::{BufWriter, Write},
    iter::Iterator,
    path::Path,
    process, result,
};

mod dwarfreader;
use binaryninja::binaryninjacore_sys::*; // TODO : Kill it with fire
use binaryninja::{
    binaryview::{BinaryView, BinaryViewBase, BinaryViewExt},
    databuffer::DataBuffer,
    Endianness,
};
use dwarfreader::DWARFReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    GimliError(gimli::Error),
    IoError,
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::result::Result<(), fmt::Error> {
        Debug::fmt(self, f)
    }
}

fn writeln_error<W: Write, R: Reader<Offset = usize>>(
    w: &mut W,
    dwarf: &Dwarf<R>,
    err: Error,
    msg: &str,
) -> io::Result<()> {
    writeln!(
        w,
        "{}: {}",
        msg,
        match err {
            Error::GimliError(err) => dwarf.format_error(err),
            Error::IoError => "An I/O error occurred while writing.".to_string(),
        }
    )
}

impl From<gimli::Error> for Error {
    fn from(err: gimli::Error) -> Self {
        Error::GimliError(err)
    }
}

impl From<io::Error> for Error {
    fn from(_: io::Error) -> Self {
        Error::IoError
    }
}

pub type Result<T> = result::Result<T, Error>;

#[derive(Default)]
struct Flags<'a> {
    eh_frame: bool,
    goff: bool,
    info: bool,
    line: bool,
    pubnames: bool,
    pubtypes: bool,
    aranges: bool,
    dwo: bool,
    dwp: bool,
    dwo_parent: Option<&'a BinaryView>,
    sup: Option<&'a BinaryView>,
    raw: bool,
    match_units: Option<Regex>,
}

fn print_usage(opts: &getopts::Options) -> ! {
    let brief = format!("Usage: {} <options> <file>", env::args().next().unwrap());
    write!(&mut io::stderr(), "{}", opts.usage(&brief)).ok();
    process::exit(1);
}

fn main() {
    binaryninja::headless::init();
    let mut opts = getopts::Options::new();
    opts.optflag(
        "",
        "eh-frame",
        "print .eh-frame exception handling frame information",
    );
    opts.optflag("G", "", "show global die offsets");
    opts.optflag("i", "", "print .debug_info and .debug_types sections");
    opts.optflag("l", "", "print .debug_line section");
    opts.optflag("p", "", "print .debug_pubnames section");
    opts.optflag("r", "", "print .debug_aranges section");
    opts.optflag("y", "", "print .debug_pubtypes section");
    opts.optflag(
        "",
        "dwo",
        "print the .dwo versions of the selected sections",
    );
    opts.optflag(
        "",
        "dwp",
        "print the .dwp versions of the selected sections",
    );
    opts.optopt(
        "",
        "dwo-parent",
        "use the specified file as the parent of the dwo or dwp (e.g. for .debug_addr)",
        "library path",
    );
    opts.optflag("", "raw", "print raw data values");
    opts.optopt(
        "u",
        "match-units",
        "print compilation units whose output matches a regex",
        "REGEX",
    );
    opts.optopt("", "sup", "path to supplementary object file", "PATH");

    let matches = match opts.parse(env::args().skip(1)) {
        Ok(m) => m,
        Err(e) => {
            writeln!(&mut io::stderr(), "{:?}\n", e).ok();
            print_usage(&opts);
        }
    };
    if matches.free.is_empty() {
        print_usage(&opts);
    }

    let mut all = true;
    let mut flags = Flags::default();
    if matches.opt_present("eh-frame") {
        flags.eh_frame = true;
        all = false;
    }
    if matches.opt_present("G") {
        flags.goff = true;
    }
    if matches.opt_present("i") {
        flags.info = true;
        all = false;
    }
    if matches.opt_present("l") {
        flags.line = true;
        all = false;
    }
    if matches.opt_present("p") {
        flags.pubnames = true;
        all = false;
    }
    if matches.opt_present("y") {
        flags.pubtypes = true;
        all = false;
    }
    if matches.opt_present("r") {
        flags.aranges = true;
        all = false;
    }
    if matches.opt_present("dwo") {
        flags.dwo = true;
    }
    if matches.opt_present("dwp") {
        flags.dwp = true;
    }
    if matches.opt_present("raw") {
        flags.raw = true;
    }
    if all {
        // .eh_frame is excluded even when printing all information.
        // cosmetic flags like -G must be set explicitly too.
        flags.info = true;
        flags.line = true;
        flags.pubnames = true;
        flags.pubtypes = true;
        flags.aranges = true;
    }
    flags.match_units = if let Some(r) = matches.opt_str("u") {
        match Regex::new(&r) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("Invalid regular expression {}: {}", r, e);
                process::exit(1);
            }
        }
    } else {
        None
    };

    for file_path in &matches.free {
        if matches.free.len() != 1 {
            println!("{}", file_path);
            println!();
        }
        match dump_file(file_path, &flags) {
            Ok(_) => (),
            Err(err) => eprintln!("Failed to dump '{}': {}", file_path, err,),
        }
    }
    binaryninja::headless::shutdown();
}

fn empty_file_section<'input, Endian: Endianity>(endian: Endian) -> DWARFReader<Endian> {
    DWARFReader::new(vec![], endian)
}

pub(crate) fn create_section_reader<'a, Endian: 'a + Endianity>(
    view: &'a BinaryView,
    endian: Endian,
    dwo_file: bool,
) -> Box<dyn Fn(SectionId) -> Result<DWARFReader<Endian>> + 'a> {
    Box::new(move |section_id: SectionId| {
        let section_name;
        if dwo_file && section_id.dwo_name().is_some() {
            section_name = section_id.dwo_name().unwrap();
        } else if dwo_file {
            println!("Ded");
            return Ok(DWARFReader::new(vec![], endian));
        } else {
            section_name = section_id.name();
        }

        println!("Querying for `{:?}`", section_name);

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
                println!("  Returning empty buffer for `{:?}`", section_name);
                return Ok(DWARFReader::new(vec![], endian));
            }
            let reader = DWARFReader::new(view.read_vec(offset, len as usize), endian);
            println!("  reader: {:?}", reader);
            return Ok(reader);
        } else {
            println!("  Returning empty buffer for `{:?}`", section_name);
            return Ok(DWARFReader::new(vec![], endian));
        }
    })
}

fn dump_file<P: AsRef<Path>>(file: P, flags: &Flags) -> Result<()> {
    let view = binaryninja::open_view(file)
        .expect("Couldn't open view")
        .parent_view()
        .unwrap();
    let endian = match view.default_endianness() {
        Endianness::LittleEndian => RunTimeEndian::Little,
        Endianness::BigEndian => RunTimeEndian::Big,
    };
    let dwo_parent = if let Some(dwo_parent_file) = flags.dwo_parent.as_ref() {
        let section_reader = create_section_reader(&dwo_parent_file, endian, false);
        Some(gimli::Dwarf::load(&section_reader)?)
    } else {
        None
    };
    let dwo_parent = dwo_parent.as_ref();

    let dwo_parent_units = if let Some(dwo_parent) = dwo_parent {
        Some(
            match dwo_parent
                .units()
                .map(|unit_header| dwo_parent.unit(unit_header))
                .filter_map(|unit| Ok(unit.dwo_id.map(|dwo_id| (dwo_id, unit))))
                .collect()
            {
                Ok(units) => units,
                Err(err) => {
                    eprintln!("Failed to process --dwo-parent units: {}", err);
                    return Ok(());
                }
            },
        )
    } else {
        None
    };
    let dwo_parent_units = dwo_parent_units.as_ref();

    let load_section = create_section_reader(&view, endian, flags.dwo || flags.dwp);

    let w = &mut BufWriter::new(io::stdout());
    if flags.dwp {
        println!("She's a DWP!");
        let empty = empty_file_section(endian);
        let dwp = gimli::DwarfPackage::load(&load_section, empty)?;
        dump_dwp(w, &dwp, dwo_parent.unwrap(), dwo_parent_units, flags)?;
        w.flush()?;
        return Ok(());
    } else {
        println!("She's _NOT_ a DWP!");
    }

    let mut dwarf = gimli::Dwarf::load(&load_section)?;
    if flags.dwo {
        println!("She's a DWO!");
        dwarf.file_type = gimli::DwarfFileType::Dwo;
        if let Some(dwo_parent) = dwo_parent {
            dwarf.debug_addr = dwo_parent.debug_addr.clone();
            dwarf
                .ranges
                .set_debug_ranges(dwo_parent.ranges.debug_ranges().clone());
        }
    } else {
        println!("She's _NOT_ a DWO!");
    }

    if let Some(sup_file) = flags.sup.as_ref() {
        println!("She's a SUP!");
        let mut load_sup_section = create_section_reader(&sup_file, endian, false);
        dwarf.load_sup(&mut load_sup_section)?;
    } else {
        println!("She's _NOT_ a SUP!");
    }

    if flags.eh_frame {
        println!("Section: eh_frame");
        let eh_frame = gimli::EhFrame::load(&load_section).unwrap();
        dump_eh_frame(w, &view, eh_frame)?;
    }
    if flags.info {
        println!("Section: info");
        dump_info(w, &dwarf, dwo_parent_units, flags)?;
        dump_types(w, &dwarf, dwo_parent_units, flags)?;
    }
    if flags.line {
        println!("Section: line");
        dump_line(w, &dwarf)?;
    }
    if flags.pubnames {
        println!("Section: pubnames");
        let debug_pubnames = &gimli::Section::load(&load_section).unwrap();
        dump_pubnames(w, debug_pubnames, &dwarf.debug_info)?;
    }
    if flags.aranges {
        println!("Section: aranges");
        let debug_aranges = &gimli::Section::load(&load_section).unwrap();
        dump_aranges(w, debug_aranges)?;
    }
    if flags.pubtypes {
        println!("Section: pubtypes");
        let debug_pubtypes = &gimli::Section::load(&load_section).unwrap();
        dump_pubtypes(w, debug_pubtypes, &dwarf.debug_info)?;
    }
    w.flush()?;
    Ok(())
}

fn dump_eh_frame<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    file: &BinaryView,
    mut eh_frame: gimli::EhFrame<R>,
) -> Result<()> {
    let address_size = file.address_size().try_into().unwrap();
    eh_frame.set_address_size(address_size);

    // fn register_name_none(_: gimli::Register) -> Option<&'static str> {
    //     None
    // }

    // TODO
    // let arch_register_name = match file.default_arch().unwrap().name().into() {
    //     "Arm" | "Aarch64" => gimli::Arm::register_name,
    //     "I386" => gimli::X86::register_name,
    //     "X86_64" => gimli::X86_64::register_name,
    //     _ => register_name_none,
    // };
    // let register_name = &|register| match arch_register_name(register) {
    //     Some(name) => Cow::Borrowed(name),
    //     None => Cow::Owned(format!("{}", register.0)),
    // };

    let mut bases = gimli::BaseAddresses::default();
    if let Ok(section) = file.section_by_name(".eh_frame_hdr") {
        bases = bases.set_eh_frame_hdr(section.start());
    }
    if let Ok(section) = file.section_by_name(".eh_frame") {
        bases = bases.set_eh_frame(section.start());
    }
    if let Ok(section) = file.section_by_name(".text") {
        bases = bases.set_text(section.start());
    }
    if let Ok(section) = file.section_by_name(".got") {
        bases = bases.set_got(section.start());
    }

    writeln!(
        w,
        "Exception handling frame information for section .eh_frame"
    )?;

    let mut cies = HashMap::new();

    let mut entries = eh_frame.entries(&bases);
    loop {
        match entries.next()? {
            None => return Ok(()),
            Some(gimli::CieOrFde::Cie(cie)) => {
                writeln!(w)?;
                writeln!(w, "{:#010x}: CIE", cie.offset())?;
                writeln!(w, "        length: {:#010x}", cie.entry_len())?;
                writeln!(w, "       version: {:#04x}", cie.version())?;
                writeln!(w, "    code_align: {}", cie.code_alignment_factor())?;
                writeln!(w, "    data_align: {}", cie.data_alignment_factor())?;
                writeln!(w, "   ra_register: {:#x}", cie.return_address_register().0)?;
                if let Some(encoding) = cie.lsda_encoding() {
                    writeln!(w, " lsda_encoding: {:#02x}", encoding.0)?;
                }
                if let Some((encoding, personality)) = cie.personality_with_encoding() {
                    write!(w, "   personality: {:#02x} ", encoding.0)?;
                    dump_pointer(w, personality)?;
                    writeln!(w)?;
                }
                if let Some(encoding) = cie.fde_address_encoding() {
                    writeln!(w, "  fde_encoding: {:#02x}", encoding.0)?;
                }
                // let instructions = cie.instructions(&eh_frame, &bases);
                // dump_cfi_instructions(w, instructions, true, register_name)?;  // TODO
                writeln!(w)?;
            }
            Some(gimli::CieOrFde::Fde(partial)) => {
                let mut offset = None;
                let fde = partial.parse(|_, bases, o| {
                    offset = Some(o);
                    cies.entry(o)
                        .or_insert_with(|| eh_frame.cie_from_offset(bases, o))
                        .clone()
                })?;

                writeln!(w)?;
                writeln!(w, "{:#010x}: FDE", fde.offset())?;
                writeln!(w, "        length: {:#010x}", fde.entry_len())?;
                writeln!(w, "   CIE_pointer: {:#010x}", offset.unwrap().0)?;
                writeln!(w, "    start_addr: {:#018x}", fde.initial_address())?;
                writeln!(
                    w,
                    "    range_size: {:#018x} (end_addr = {:#018x})",
                    fde.len(),
                    fde.initial_address() + fde.len()
                )?;
                if let Some(lsda) = fde.lsda() {
                    write!(w, "          lsda: ")?;
                    dump_pointer(w, lsda)?;
                    writeln!(w)?;
                }
                // let instructions = fde.instructions(&eh_frame, &bases);
                // dump_cfi_instructions(w, instructions, false, register_name)?;  // TODO
                writeln!(w)?;
            }
        }
    }
}

fn dump_pointer<W: Write>(w: &mut W, p: gimli::Pointer) -> Result<()> {
    match p {
        gimli::Pointer::Direct(p) => {
            write!(w, "{:#018x}", p)?;
        }
        gimli::Pointer::Indirect(p) => {
            write!(w, "({:#018x})", p)?;
        }
    }
    Ok(())
}

// #[allow(clippy::unneeded_field_pattern)]
// fn dump_cfi_instructions<R: Reader<Offset = usize>, W: Write>(
//     w: &mut W,
//     mut insns: gimli::CallFrameInstructionIter<R>,
//     is_initial: bool,
//     register_name: &dyn Fn(gimli::Register) -> Cow<'static, str>,
// ) -> Result<()> {
//     use gimli::CallFrameInstruction::*;

//     // TODO: we need to actually evaluate these instructions as we iterate them
//     // so we can print the initialized state for CIEs, and each unwind row's
//     // registers for FDEs.
//     //
//     // TODO: We should print DWARF expressions for the CFI instructions that
//     // embed DWARF expressions within themselves.

//     if !is_initial {
//         writeln!(w, "  Instructions:")?;
//     }

//     loop {
//         match insns.next() {
//             Err(e) => {
//                 writeln!(w, "Failed to decode CFI instruction: {}", e)?;
//                 return Ok(());
//             }
//             Ok(None) => {
//                 if is_initial {
//                     writeln!(w, "  Instructions: Init State:")?;
//                 }
//                 return Ok(());
//             }
//             Ok(Some(op)) => match op {
//                 SetLoc { address } => {
//                     writeln!(w, "                DW_CFA_set_loc ({:#x})", address)?;
//                 }
//                 AdvanceLoc { delta } => {
//                     writeln!(w, "                DW_CFA_advance_loc ({})", delta)?;
//                 }
//                 DefCfa { register, offset } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_def_cfa ({}, {})",
//                         register_name(register),
//                         offset
//                     )?;
//                 }
//                 DefCfaSf {
//                     register,
//                     factored_offset,
//                 } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_def_cfa_sf ({}, {})",
//                         register_name(register),
//                         factored_offset
//                     )?;
//                 }
//                 DefCfaRegister { register } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_def_cfa_register ({})",
//                         register_name(register)
//                     )?;
//                 }
//                 DefCfaOffset { offset } => {
//                     writeln!(w, "                DW_CFA_def_cfa_offset ({})", offset)?;
//                 }
//                 DefCfaOffsetSf { factored_offset } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_def_cfa_offset_sf ({})",
//                         factored_offset
//                     )?;
//                 }
//                 DefCfaExpression { expression: _ } => {
//                     writeln!(w, "                DW_CFA_def_cfa_expression (...)")?;
//                 }
//                 Undefined { register } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_undefined ({})",
//                         register_name(register)
//                     )?;
//                 }
//                 SameValue { register } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_same_value ({})",
//                         register_name(register)
//                     )?;
//                 }
//                 Offset {
//                     register,
//                     factored_offset,
//                 } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_offset ({}, {})",
//                         register_name(register),
//                         factored_offset
//                     )?;
//                 }
//                 OffsetExtendedSf {
//                     register,
//                     factored_offset,
//                 } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_offset_extended_sf ({}, {})",
//                         register_name(register),
//                         factored_offset
//                     )?;
//                 }
//                 ValOffset {
//                     register,
//                     factored_offset,
//                 } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_val_offset ({}, {})",
//                         register_name(register),
//                         factored_offset
//                     )?;
//                 }
//                 ValOffsetSf {
//                     register,
//                     factored_offset,
//                 } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_val_offset_sf ({}, {})",
//                         register_name(register),
//                         factored_offset
//                     )?;
//                 }
//                 Register {
//                     dest_register,
//                     src_register,
//                 } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_register ({}, {})",
//                         register_name(dest_register),
//                         register_name(src_register)
//                     )?;
//                 }
//                 Expression {
//                     register,
//                     expression: _,
//                 } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_expression ({}, ...)",
//                         register_name(register)
//                     )?;
//                 }
//                 ValExpression {
//                     register,
//                     expression: _,
//                 } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_val_expression ({}, ...)",
//                         register_name(register)
//                     )?;
//                 }
//                 Restore { register } => {
//                     writeln!(
//                         w,
//                         "                DW_CFA_restore ({})",
//                         register_name(register)
//                     )?;
//                 }
//                 RememberState => {
//                     writeln!(w, "                DW_CFA_remember_state")?;
//                 }
//                 RestoreState => {
//                     writeln!(w, "                DW_CFA_restore_state")?;
//                 }
//                 ArgsSize { size } => {
//                     writeln!(w, "                DW_CFA_GNU_args_size ({})", size)?;
//                 }
//                 Nop => {
//                     writeln!(w, "                DW_CFA_nop")?;
//                 }
//             },
//         }
//     }
// }

fn dump_dwp<R: Reader<Offset = usize>, W: Write + Send>(
    w: &mut W,
    dwp: &gimli::DwarfPackage<R>,
    dwo_parent: &gimli::Dwarf<R>,
    dwo_parent_units: Option<&HashMap<gimli::DwoId, gimli::Unit<R>>>,
    flags: &Flags,
) -> Result<()>
where
    R::Endian: Send + Sync,
{
    if dwp.cu_index.unit_count() != 0 {
        writeln!(
            w,
            "\n.debug_cu_index: version = {}, sections = {}, units = {}, slots = {}",
            dwp.cu_index.version(),
            dwp.cu_index.section_count(),
            dwp.cu_index.unit_count(),
            dwp.cu_index.slot_count(),
        )?;
        for i in 1..=dwp.cu_index.unit_count() {
            writeln!(w, "\nCU index {}", i)?;
            dump_dwp_sections(
                w,
                &dwp,
                dwo_parent,
                dwo_parent_units,
                flags,
                dwp.cu_index.sections(i)?,
            )?;
        }
    }

    if dwp.tu_index.unit_count() != 0 {
        writeln!(
            w,
            "\n.debug_tu_index: version = {}, sections = {}, units = {}, slots = {}",
            dwp.tu_index.version(),
            dwp.tu_index.section_count(),
            dwp.tu_index.unit_count(),
            dwp.tu_index.slot_count(),
        )?;
        for i in 1..=dwp.tu_index.unit_count() {
            writeln!(w, "\nTU index {}", i)?;
            dump_dwp_sections(
                w,
                &dwp,
                dwo_parent,
                dwo_parent_units,
                flags,
                dwp.tu_index.sections(i)?,
            )?;
        }
    }

    Ok(())
}

fn dump_dwp_sections<R: Reader<Offset = usize>, W: Write + Send>(
    w: &mut W,
    dwp: &gimli::DwarfPackage<R>,
    dwo_parent: &gimli::Dwarf<R>,
    dwo_parent_units: Option<&HashMap<gimli::DwoId, gimli::Unit<R>>>,
    flags: &Flags,
    sections: gimli::UnitIndexSectionIterator<R>,
) -> Result<()> {
    for section in sections.clone() {
        writeln!(
            w,
            "  {}: offset = 0x{:x}, size = 0x{:x}",
            section.section.dwo_name().unwrap(),
            section.offset,
            section.size
        )?;
    }
    let dwarf = dwp.sections(sections, dwo_parent)?;
    if flags.info {
        dump_info(w, &dwarf, dwo_parent_units, flags)?;
        dump_types(w, &dwarf, dwo_parent_units, flags)?;
    }
    if flags.line {
        dump_line(w, &dwarf)?;
    }
    Ok(())
}

fn dump_info<R: Reader<Offset = usize>, W: Write + Send>(
    w: &mut W,
    dwarf: &gimli::Dwarf<R>,
    dwo_parent_units: Option<&HashMap<gimli::DwoId, gimli::Unit<R>>>,
    flags: &Flags,
) -> Result<()> {
    writeln!(w, "\n.debug_info")?;

    let units = match dwarf.units().collect::<Vec<_>>() {
        Ok(units) => units,
        Err(err) => {
            writeln_error(
                w,
                dwarf,
                Error::GimliError(err),
                "Failed to read unit headers",
            )?;
            return Ok(());
        }
    };

    for unit in units {
        match dump_unit(w, unit, dwarf, dwo_parent_units, flags) {
            Ok(_) => (),
            e => return e,
        }
    }
    Ok(())
}

fn dump_types<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    dwarf: &gimli::Dwarf<R>,
    dwo_parent_units: Option<&HashMap<gimli::DwoId, gimli::Unit<R>>>,
    flags: &Flags,
) -> Result<()> {
    writeln!(w, "\n.debug_types")?;

    let mut iter = dwarf.type_units();
    while let Some(header) = iter.next()? {
        dump_unit(w, header, dwarf, dwo_parent_units, flags)?;
    }
    Ok(())
}

fn dump_unit<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    header: UnitHeader<R>,
    dwarf: &gimli::Dwarf<R>,
    dwo_parent_units: Option<&HashMap<gimli::DwoId, gimli::Unit<R>>>,
    flags: &Flags,
) -> Result<()> {
    write!(w, "\nUNIT<")?;
    match header.offset() {
        UnitSectionOffset::DebugInfoOffset(o) => {
            write!(w, ".debug_info+0x{:08x}", o.0)?;
        }
        UnitSectionOffset::DebugTypesOffset(o) => {
            write!(w, ".debug_types+0x{:08x}", o.0)?;
        }
    }
    writeln!(w, ">: length = 0x{:x}, format = {:?}, version = {}, address_size = {}, abbrev_offset = 0x{:x}",
        header.unit_length(),
        header.format(),
        header.version(),
        header.address_size(),
        header.debug_abbrev_offset().0,
    )?;

    match header.type_() {
        UnitType::Compilation | UnitType::Partial => (),
        UnitType::Type {
            type_signature,
            type_offset,
        }
        | UnitType::SplitType {
            type_signature,
            type_offset,
        } => {
            write!(w, "  signature        = ")?;
            dump_type_signature(w, type_signature)?;
            writeln!(w)?;
            writeln!(w, "  type_offset      = 0x{:x}", type_offset.0,)?;
        }
        UnitType::Skeleton(dwo_id) | UnitType::SplitCompilation(dwo_id) => {
            write!(w, "  dwo_id           = ")?;
            writeln!(w, "0x{:016x}", dwo_id.0)?;
        }
    }

    let mut unit = match dwarf.unit(header) {
        Ok(unit) => unit,
        Err(err) => {
            writeln_error(w, dwarf, err.into(), "Failed to parse unit root entry")?;
            return Ok(());
        }
    };

    if let Some(dwo_parent_units) = dwo_parent_units {
        if let Some(dwo_id) = unit.dwo_id {
            if let Some(parent_unit) = dwo_parent_units.get(&dwo_id) {
                unit.copy_relocated_attributes(parent_unit);
            }
        }
    }

    let entries_result = dump_entries(w, unit, dwarf, flags);
    if let Err(err) = entries_result {
        writeln_error(w, dwarf, err, "Failed to dump entries")?;
    }
    Ok(())
}

fn spaces(buf: &mut String, len: usize) -> &str {
    while buf.len() < len {
        buf.push(' ');
    }
    &buf[..len]
}

// " GOFF=0x{:08x}" adds exactly 16 spaces.
const GOFF_SPACES: usize = 16;

fn write_offset<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    unit: &gimli::Unit<R>,
    offset: gimli::UnitOffset<R::Offset>,
    flags: &Flags,
) -> Result<()> {
    write!(w, "<0x{:08x}", offset.0)?;
    if flags.goff {
        let goff = match offset.to_unit_section_offset(unit) {
            UnitSectionOffset::DebugInfoOffset(o) => o.0,
            UnitSectionOffset::DebugTypesOffset(o) => o.0,
        };
        write!(w, " GOFF=0x{:08x}", goff)?;
    }
    write!(w, ">")?;
    Ok(())
}

fn dump_entries<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    unit: gimli::Unit<R>,
    dwarf: &gimli::Dwarf<R>,
    flags: &Flags,
) -> Result<()> {
    let mut spaces_buf = String::new();

    let mut entries = unit.entries_raw(None)?;
    while !entries.is_empty() {
        let offset = entries.next_offset();
        let depth = entries.next_depth();
        let abbrev = entries.read_abbreviation()?;

        let mut indent = if depth >= 0 {
            depth as usize * 2 + 2
        } else {
            2
        };
        write!(w, "<{}{}>", if depth < 10 { " " } else { "" }, depth)?;
        write_offset(w, &unit, offset, flags)?;
        writeln!(
            w,
            "{}{}",
            spaces(&mut spaces_buf, indent),
            abbrev.map(|x| x.tag()).unwrap_or(gimli::DW_TAG_null)
        )?;

        indent += 18;
        if flags.goff {
            indent += GOFF_SPACES;
        }

        for spec in abbrev.map(|x| x.attributes()).unwrap_or(&[]) {
            let attr = entries.read_attribute(*spec)?;
            w.write_all(spaces(&mut spaces_buf, indent).as_bytes())?;
            if let Some(n) = attr.name().static_string() {
                let right_padding = 27 - std::cmp::min(27, n.len());
                write!(w, "{}{} ", n, spaces(&mut spaces_buf, right_padding))?;
            } else {
                write!(w, "{:27} ", attr.name())?;
            }
            if flags.raw {
                writeln!(w, "{:?}", attr.raw_value())?;
            } else {
                match dump_attr_value(w, &attr, &unit, dwarf) {
                    Ok(_) => (),
                    Err(err) => writeln_error(w, dwarf, err, "Failed to dump attribute value")?,
                };
            }
        }
    }
    Ok(())
}

fn dump_attr_value<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    attr: &gimli::Attribute<R>,
    unit: &gimli::Unit<R>,
    dwarf: &gimli::Dwarf<R>,
) -> Result<()> {
    let value = attr.value();
    match value {
        gimli::AttributeValue::Addr(address) => {
            writeln!(w, "0x{:08x}", address)?;
        }
        gimli::AttributeValue::Block(data) => {
            for byte in data.to_slice()?.iter() {
                write!(w, "{:02x}", byte)?;
            }
            writeln!(w)?;
        }
        gimli::AttributeValue::Data1(_)
        | gimli::AttributeValue::Data2(_)
        | gimli::AttributeValue::Data4(_)
        | gimli::AttributeValue::Data8(_) => {
            if let (Some(udata), Some(sdata)) = (attr.udata_value(), attr.sdata_value()) {
                if sdata >= 0 {
                    writeln!(w, "{}", udata)?;
                } else {
                    writeln!(w, "{} ({})", udata, sdata)?;
                }
            } else {
                writeln!(w, "{:?}", value)?;
            }
        }
        gimli::AttributeValue::Sdata(data) => {
            match attr.name() {
                gimli::DW_AT_data_member_location => {
                    writeln!(w, "{}", data)?;
                }
                _ => {
                    if data >= 0 {
                        writeln!(w, "0x{:08x}", data)?;
                    } else {
                        writeln!(w, "0x{:08x} ({})", data, data)?;
                    }
                }
            };
        }
        gimli::AttributeValue::Udata(data) => {
            match attr.name() {
                gimli::DW_AT_high_pc => {
                    writeln!(w, "<offset-from-lowpc>{}", data)?;
                }
                gimli::DW_AT_data_member_location => {
                    if let Some(sdata) = attr.sdata_value() {
                        // This is a DW_FORM_data* value.
                        // libdwarf-dwarfdump displays this as signed too.
                        if sdata >= 0 {
                            writeln!(w, "{}", data)?;
                        } else {
                            writeln!(w, "{} ({})", data, sdata)?;
                        }
                    } else {
                        writeln!(w, "{}", data)?;
                    }
                }
                gimli::DW_AT_lower_bound | gimli::DW_AT_upper_bound => {
                    writeln!(w, "{}", data)?;
                }
                _ => {
                    writeln!(w, "0x{:08x}", data)?;
                }
            };
        }
        gimli::AttributeValue::Exprloc(ref data) => {
            if let gimli::AttributeValue::Exprloc(_) = attr.raw_value() {
                write!(w, "len 0x{:04x}: ", data.0.len())?;
                for byte in data.0.to_slice()?.iter() {
                    write!(w, "{:02x}", byte)?;
                }
                write!(w, ": ")?;
            }
            dump_exprloc(w, unit.encoding(), data)?;
            writeln!(w)?;
        }
        gimli::AttributeValue::Flag(true) => {
            writeln!(w, "yes")?;
        }
        gimli::AttributeValue::Flag(false) => {
            writeln!(w, "no")?;
        }
        gimli::AttributeValue::SecOffset(offset) => {
            writeln!(w, "0x{:08x}", offset)?;
        }
        gimli::AttributeValue::DebugAddrBase(base) => {
            writeln!(w, "<.debug_addr+0x{:08x}>", base.0)?;
        }
        gimli::AttributeValue::DebugAddrIndex(index) => {
            write!(w, "(indirect address, index {:#x}): ", index.0)?;
            let address = dwarf.address(unit, index)?;
            writeln!(w, "0x{:08x}", address)?;
        }
        gimli::AttributeValue::UnitRef(offset) => {
            write!(w, "0x{:08x}", offset.0)?;
            match offset.to_unit_section_offset(unit) {
                UnitSectionOffset::DebugInfoOffset(goff) => {
                    write!(w, "<.debug_info+0x{:08x}>", goff.0)?;
                }
                UnitSectionOffset::DebugTypesOffset(goff) => {
                    write!(w, "<.debug_types+0x{:08x}>", goff.0)?;
                }
            }
            writeln!(w)?;
        }
        gimli::AttributeValue::DebugInfoRef(offset) => {
            writeln!(w, "<.debug_info+0x{:08x}>", offset.0)?;
        }
        gimli::AttributeValue::DebugInfoRefSup(offset) => {
            writeln!(w, "<.debug_info(sup)+0x{:08x}>", offset.0)?;
        }
        gimli::AttributeValue::DebugLineRef(offset) => {
            writeln!(w, "<.debug_line+0x{:08x}>", offset.0)?;
        }
        gimli::AttributeValue::LocationListsRef(offset) => {
            dump_loc_list(w, offset, unit, dwarf)?;
        }
        gimli::AttributeValue::DebugLocListsBase(base) => {
            writeln!(w, "<.debug_loclists+0x{:08x}>", base.0)?;
        }
        gimli::AttributeValue::DebugLocListsIndex(index) => {
            write!(w, "(indirect location list, index {:#x}): ", index.0)?;
            let offset = dwarf.locations_offset(unit, index)?;
            dump_loc_list(w, offset, unit, dwarf)?;
        }
        gimli::AttributeValue::DebugMacinfoRef(offset) => {
            writeln!(w, "<.debug_macinfo+0x{:08x}>", offset.0)?;
        }
        gimli::AttributeValue::DebugMacroRef(offset) => {
            writeln!(w, "<.debug_macro+0x{:08x}>", offset.0)?;
        }
        gimli::AttributeValue::RangeListsRef(offset) => {
            let offset = dwarf.ranges_offset_from_raw(unit, offset);
            dump_range_list(w, offset, unit, dwarf)?;
        }
        gimli::AttributeValue::DebugRngListsBase(base) => {
            writeln!(w, "<.debug_rnglists+0x{:08x}>", base.0)?;
        }
        gimli::AttributeValue::DebugRngListsIndex(index) => {
            write!(w, "(indirect range list, index {:#x}): ", index.0)?;
            let offset = dwarf.ranges_offset(unit, index)?;
            dump_range_list(w, offset, unit, dwarf)?;
        }
        gimli::AttributeValue::DebugTypesRef(signature) => {
            dump_type_signature(w, signature)?;
            writeln!(w, " <type signature>")?;
        }
        gimli::AttributeValue::DebugStrRef(offset) => {
            if let Ok(s) = dwarf.debug_str.get_str(offset) {
                writeln!(w, "{}", s.to_string_lossy()?)?;
            } else {
                writeln!(w, "<.debug_str+0x{:08x}>", offset.0)?;
            }
        }
        gimli::AttributeValue::DebugStrRefSup(offset) => {
            if let Some(s) = dwarf
                .sup()
                .and_then(|sup| sup.debug_str.get_str(offset).ok())
            {
                writeln!(w, "{}", s.to_string_lossy()?)?;
            } else {
                writeln!(w, "<.debug_str(sup)+0x{:08x}>", offset.0)?;
            }
        }
        gimli::AttributeValue::DebugStrOffsetsBase(base) => {
            writeln!(w, "<.debug_str_offsets+0x{:08x}>", base.0)?;
        }
        gimli::AttributeValue::DebugStrOffsetsIndex(index) => {
            write!(w, "(indirect string, index {:#x}): ", index.0)?;
            let offset = dwarf.debug_str_offsets.get_str_offset(
                unit.encoding().format,
                unit.str_offsets_base,
                index,
            )?;
            if let Ok(s) = dwarf.debug_str.get_str(offset) {
                writeln!(w, "{}", s.to_string_lossy()?)?;
            } else {
                writeln!(w, "<.debug_str+0x{:08x}>", offset.0)?;
            }
        }
        gimli::AttributeValue::DebugLineStrRef(offset) => {
            if let Ok(s) = dwarf.debug_line_str.get_str(offset) {
                writeln!(w, "{}", s.to_string_lossy()?)?;
            } else {
                writeln!(w, "<.debug_line_str=0x{:08x}>", offset.0)?;
            }
        }
        gimli::AttributeValue::String(s) => {
            writeln!(w, "{}", s.to_string_lossy()?)?;
        }
        gimli::AttributeValue::Encoding(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::DecimalSign(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::Endianity(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::Accessibility(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::Visibility(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::Virtuality(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::Language(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::AddressClass(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::IdentifierCase(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::CallingConvention(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::Inline(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::Ordering(value) => {
            writeln!(w, "{}", value)?;
        }
        gimli::AttributeValue::FileIndex(value) => {
            write!(w, "0x{:08x}", value)?;
            dump_file_index(w, value, unit, dwarf)?;
            writeln!(w)?;
        }
        gimli::AttributeValue::DwoId(value) => {
            writeln!(w, "0x{:016x}", value.0)?;
        }
    }

    Ok(())
}

fn dump_type_signature<W: Write>(w: &mut W, signature: gimli::DebugTypeSignature) -> Result<()> {
    write!(w, "0x{:016x}", signature.0)?;
    Ok(())
}

fn dump_file_index<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    file_index: u64,
    unit: &gimli::Unit<R>,
    dwarf: &gimli::Dwarf<R>,
) -> Result<()> {
    if file_index == 0 && unit.header.version() <= 4 {
        return Ok(());
    }
    let header = match unit.line_program {
        Some(ref program) => program.header(),
        None => return Ok(()),
    };
    let file = match header.file(file_index) {
        Some(file) => file,
        None => {
            writeln!(w, "Unable to get header for file {}", file_index)?;
            return Ok(());
        }
    };
    write!(w, " ")?;
    if let Some(directory) = file.directory(header) {
        let directory = dwarf.attr_string(unit, directory)?;
        let directory = directory.to_string_lossy()?;
        if file.directory_index() != 0 && !directory.starts_with('/') {
            if let Some(ref comp_dir) = unit.comp_dir {
                write!(w, "{}/", comp_dir.to_string_lossy()?,)?;
            }
        }
        write!(w, "{}/", directory)?;
    }
    write!(
        w,
        "{}",
        dwarf
            .attr_string(unit, file.path_name())?
            .to_string_lossy()?
    )?;
    Ok(())
}

fn dump_exprloc<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    encoding: gimli::Encoding,
    data: &gimli::Expression<R>,
) -> Result<()> {
    let mut pc = data.0.clone();
    let mut space = false;
    while pc.len() != 0 {
        let pc_clone = pc.clone();
        match gimli::Operation::parse(&mut pc, encoding) {
            Ok(op) => {
                if space {
                    write!(w, " ")?;
                } else {
                    space = true;
                }
                dump_op(w, encoding, pc_clone, op)?;
            }
            Err(gimli::Error::InvalidExpression(op)) => {
                writeln!(w, "WARNING: unsupported operation 0x{:02x}", op.0)?;
                return Ok(());
            }
            Err(gimli::Error::UnsupportedRegister(register)) => {
                writeln!(w, "WARNING: unsupported register {}", register)?;
                return Ok(());
            }
            Err(gimli::Error::UnexpectedEof(_)) => {
                writeln!(w, "WARNING: truncated or malformed expression")?;
                return Ok(());
            }
            Err(e) => {
                writeln!(w, "WARNING: unexpected operation parse error: {}", e)?;
                return Ok(());
            }
        }
    }
    Ok(())
}

fn dump_op<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    encoding: gimli::Encoding,
    mut pc: R,
    op: gimli::Operation<R>,
) -> Result<()> {
    let dwop = gimli::DwOp(pc.read_u8()?);
    write!(w, "{}", dwop)?;
    match op {
        gimli::Operation::Deref {
            base_type, size, ..
        } => {
            if dwop == gimli::DW_OP_deref_size || dwop == gimli::DW_OP_xderef_size {
                write!(w, " {}", size)?;
            }
            if base_type != UnitOffset(0) {
                write!(w, " type 0x{:08x}", base_type.0)?;
            }
        }
        gimli::Operation::Pick { index } => {
            if dwop == gimli::DW_OP_pick {
                write!(w, " {}", index)?;
            }
        }
        gimli::Operation::PlusConstant { value } => {
            write!(w, " {}", value as i64)?;
        }
        gimli::Operation::Bra { target } => {
            write!(w, " {}", target)?;
        }
        gimli::Operation::Skip { target } => {
            write!(w, " {}", target)?;
        }
        gimli::Operation::SignedConstant { value } => match dwop {
            gimli::DW_OP_const1s
            | gimli::DW_OP_const2s
            | gimli::DW_OP_const4s
            | gimli::DW_OP_const8s
            | gimli::DW_OP_consts => {
                write!(w, " {}", value)?;
            }
            _ => {}
        },
        gimli::Operation::UnsignedConstant { value } => match dwop {
            gimli::DW_OP_const1u
            | gimli::DW_OP_const2u
            | gimli::DW_OP_const4u
            | gimli::DW_OP_const8u
            | gimli::DW_OP_constu => {
                write!(w, " {}", value)?;
            }
            _ => {
                // These have the value encoded in the operation, eg DW_OP_lit0.
            }
        },
        gimli::Operation::Register { register } => {
            if dwop == gimli::DW_OP_regx {
                write!(w, " {}", register.0)?;
            }
        }
        gimli::Operation::RegisterOffset {
            register,
            offset,
            base_type,
        } => {
            if dwop >= gimli::DW_OP_breg0 && dwop <= gimli::DW_OP_breg31 {
                write!(w, "{:+}", offset)?;
            } else {
                write!(w, " {}", register.0)?;
                if offset != 0 {
                    write!(w, "{:+}", offset)?;
                }
                if base_type != UnitOffset(0) {
                    write!(w, " type 0x{:08x}", base_type.0)?;
                }
            }
        }
        gimli::Operation::FrameOffset { offset } => {
            write!(w, " {}", offset)?;
        }
        gimli::Operation::Call { offset } => match offset {
            gimli::DieReference::UnitRef(gimli::UnitOffset(offset)) => {
                write!(w, " 0x{:08x}", offset)?;
            }
            gimli::DieReference::DebugInfoRef(gimli::DebugInfoOffset(offset)) => {
                write!(w, " 0x{:08x}", offset)?;
            }
        },
        gimli::Operation::Piece {
            size_in_bits,
            bit_offset: None,
        } => {
            write!(w, " {}", size_in_bits / 8)?;
        }
        gimli::Operation::Piece {
            size_in_bits,
            bit_offset: Some(bit_offset),
        } => {
            write!(w, " 0x{:08x} offset 0x{:08x}", size_in_bits, bit_offset)?;
        }
        gimli::Operation::ImplicitValue { data } => {
            let data = data.to_slice()?;
            write!(w, " 0x{:08x} contents 0x", data.len())?;
            for byte in data.iter() {
                write!(w, "{:02x}", byte)?;
            }
        }
        gimli::Operation::ImplicitPointer { value, byte_offset } => {
            write!(w, " 0x{:08x} {}", value.0, byte_offset)?;
        }
        gimli::Operation::EntryValue { expression } => {
            write!(w, "(")?;
            dump_exprloc(w, encoding, &gimli::Expression(expression))?;
            write!(w, ")")?;
        }
        gimli::Operation::ParameterRef { offset } => {
            write!(w, " 0x{:08x}", offset.0)?;
        }
        gimli::Operation::Address { address } => {
            write!(w, " 0x{:08x}", address)?;
        }
        gimli::Operation::AddressIndex { index } => {
            write!(w, " 0x{:08x}", index.0)?;
        }
        gimli::Operation::ConstantIndex { index } => {
            write!(w, " 0x{:08x}", index.0)?;
        }
        gimli::Operation::TypedLiteral { base_type, value } => {
            write!(w, " type 0x{:08x} contents 0x", base_type.0)?;
            for byte in value.to_slice()?.iter() {
                write!(w, "{:02x}", byte)?;
            }
        }
        gimli::Operation::Convert { base_type } => {
            write!(w, " type 0x{:08x}", base_type.0)?;
        }
        gimli::Operation::Reinterpret { base_type } => {
            write!(w, " type 0x{:08x}", base_type.0)?;
        }
        gimli::Operation::WasmLocal { index }
        | gimli::Operation::WasmGlobal { index }
        | gimli::Operation::WasmStack { index } => {
            let wasmop = pc.read_u8()?;
            write!(w, " 0x{:x} 0x{:x}", wasmop, index)?;
        }
        gimli::Operation::Drop
        | gimli::Operation::Swap
        | gimli::Operation::Rot
        | gimli::Operation::Abs
        | gimli::Operation::And
        | gimli::Operation::Div
        | gimli::Operation::Minus
        | gimli::Operation::Mod
        | gimli::Operation::Mul
        | gimli::Operation::Neg
        | gimli::Operation::Not
        | gimli::Operation::Or
        | gimli::Operation::Plus
        | gimli::Operation::Shl
        | gimli::Operation::Shr
        | gimli::Operation::Shra
        | gimli::Operation::Xor
        | gimli::Operation::Eq
        | gimli::Operation::Ge
        | gimli::Operation::Gt
        | gimli::Operation::Le
        | gimli::Operation::Lt
        | gimli::Operation::Ne
        | gimli::Operation::Nop
        | gimli::Operation::PushObjectAddress
        | gimli::Operation::TLS
        | gimli::Operation::CallFrameCFA
        | gimli::Operation::StackValue => {}
    };
    Ok(())
}

fn dump_loc_list<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    offset: gimli::LocationListsOffset<R::Offset>,
    unit: &gimli::Unit<R>,
    dwarf: &gimli::Dwarf<R>,
) -> Result<()> {
    let raw_locations = dwarf.raw_locations(unit, offset)?;
    let raw_locations: Vec<_> = raw_locations.collect()?;
    let mut locations = dwarf.locations(unit, offset)?;
    writeln!(
        w,
        "<loclist at {}+0x{:08x} with {} entries>",
        if unit.encoding().version < 5 {
            ".debug_loc"
        } else {
            ".debug_loclists"
        },
        offset.0,
        raw_locations.len()
    )?;
    for (i, raw) in raw_locations.iter().enumerate() {
        write!(w, "\t\t\t[{:2}]", i)?;
        match *raw {
            gimli::RawLocListEntry::BaseAddress { addr } => {
                writeln!(w, "<new base address 0x{:08x}>", addr)?;
            }
            gimli::RawLocListEntry::BaseAddressx { addr } => {
                let addr_val = dwarf.address(unit, addr)?;
                writeln!(w, "<new base addressx [{}]0x{:08x}>", addr.0, addr_val)?;
            }
            gimli::RawLocListEntry::StartxEndx {
                begin,
                end,
                ref data,
            } => {
                let begin_val = dwarf.address(unit, begin)?;
                let end_val = dwarf.address(unit, end)?;
                let location = locations.next()?.unwrap();
                write!(
                    w,
                    "<startx-endx \
                     low-off: [{}]0x{:08x} addr 0x{:08x} \
                     high-off: [{}]0x{:08x} addr 0x{:08x}>",
                    begin.0, begin_val, location.range.begin, end.0, end_val, location.range.end
                )?;
                dump_exprloc(w, unit.encoding(), data)?;
                writeln!(w)?;
            }
            gimli::RawLocListEntry::StartxLength {
                begin,
                length,
                ref data,
            } => {
                let begin_val = dwarf.address(unit, begin)?;
                let location = locations.next()?.unwrap();
                write!(
                    w,
                    "<start-length \
                     low-off: [{}]0x{:08x} addr 0x{:08x} \
                     high-off: 0x{:08x} addr 0x{:08x}>",
                    begin.0, begin_val, location.range.begin, length, location.range.end
                )?;
                dump_exprloc(w, unit.encoding(), data)?;
                writeln!(w)?;
            }
            gimli::RawLocListEntry::AddressOrOffsetPair {
                begin,
                end,
                ref data,
            }
            | gimli::RawLocListEntry::OffsetPair {
                begin,
                end,
                ref data,
            } => {
                let location = locations.next()?.unwrap();
                write!(
                    w,
                    "<offset pair \
                     low-off: 0x{:08x} addr 0x{:08x} \
                     high-off: 0x{:08x} addr 0x{:08x}>",
                    begin, location.range.begin, end, location.range.end
                )?;
                dump_exprloc(w, unit.encoding(), data)?;
                writeln!(w)?;
            }
            gimli::RawLocListEntry::DefaultLocation { ref data } => {
                write!(w, "<default location>")?;
                dump_exprloc(w, unit.encoding(), data)?;
                writeln!(w)?;
            }
            gimli::RawLocListEntry::StartEnd {
                begin,
                end,
                ref data,
            } => {
                let location = locations.next()?.unwrap();
                write!(
                    w,
                    "<start-end \
                     low-off: 0x{:08x} addr 0x{:08x} \
                     high-off: 0x{:08x} addr 0x{:08x}>",
                    begin, location.range.begin, end, location.range.end
                )?;
                dump_exprloc(w, unit.encoding(), data)?;
                writeln!(w)?;
            }
            gimli::RawLocListEntry::StartLength {
                begin,
                length,
                ref data,
            } => {
                let location = locations.next()?.unwrap();
                write!(
                    w,
                    "<start-length \
                     low-off: 0x{:08x} addr 0x{:08x} \
                     high-off: 0x{:08x} addr 0x{:08x}>",
                    begin, location.range.begin, length, location.range.end
                )?;
                dump_exprloc(w, unit.encoding(), data)?;
                writeln!(w)?;
            }
        };
    }
    Ok(())
}

fn dump_range_list<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    offset: gimli::RangeListsOffset<R::Offset>,
    unit: &gimli::Unit<R>,
    dwarf: &gimli::Dwarf<R>,
) -> Result<()> {
    let raw_ranges = dwarf.raw_ranges(unit, offset)?;
    let raw_ranges: Vec<_> = raw_ranges.collect()?;
    let mut ranges = dwarf.ranges(unit, offset)?;
    writeln!(
        w,
        "<rnglist at {}+0x{:08x} with {} entries>",
        if unit.encoding().version < 5 {
            ".debug_ranges"
        } else {
            ".debug_rnglists"
        },
        offset.0,
        raw_ranges.len()
    )?;
    for (i, raw) in raw_ranges.iter().enumerate() {
        write!(w, "\t\t\t[{:2}] ", i)?;
        match *raw {
            gimli::RawRngListEntry::AddressOrOffsetPair { begin, end } => {
                let range = ranges.next()?.unwrap();
                writeln!(
                    w,
                    "<address pair \
                     low-off: 0x{:08x} addr 0x{:08x} \
                     high-off: 0x{:08x} addr 0x{:08x}>",
                    begin, range.begin, end, range.end
                )?;
            }
            gimli::RawRngListEntry::BaseAddress { addr } => {
                writeln!(w, "<new base address 0x{:08x}>", addr)?;
            }
            gimli::RawRngListEntry::BaseAddressx { addr } => {
                let addr_val = dwarf.address(unit, addr)?;
                writeln!(w, "<new base addressx [{}]0x{:08x}>", addr.0, addr_val)?;
            }
            gimli::RawRngListEntry::StartxEndx { begin, end } => {
                let begin_val = dwarf.address(unit, begin)?;
                let end_val = dwarf.address(unit, end)?;
                let range = if begin_val == end_val {
                    gimli::Range {
                        begin: begin_val,
                        end: end_val,
                    }
                } else {
                    ranges.next()?.unwrap()
                };
                writeln!(
                    w,
                    "<startx-endx \
                     low-off: [{}]0x{:08x} addr 0x{:08x} \
                     high-off: [{}]0x{:08x} addr 0x{:08x}>",
                    begin.0, begin_val, range.begin, end.0, end_val, range.end
                )?;
            }
            gimli::RawRngListEntry::StartxLength { begin, length } => {
                let begin_val = dwarf.address(unit, begin)?;
                let range = ranges.next()?.unwrap();
                writeln!(
                    w,
                    "<startx-length \
                     low-off: [{}]0x{:08x} addr 0x{:08x} \
                     high-off: 0x{:08x} addr 0x{:08x}>",
                    begin.0, begin_val, range.begin, length, range.end
                )?;
            }
            gimli::RawRngListEntry::OffsetPair { begin, end } => {
                let range = ranges.next()?.unwrap();
                writeln!(
                    w,
                    "<offset pair \
                     low-off: 0x{:08x} addr 0x{:08x} \
                     high-off: 0x{:08x} addr 0x{:08x}>",
                    begin, range.begin, end, range.end
                )?;
            }
            gimli::RawRngListEntry::StartEnd { begin, end } => {
                let range = if begin == end {
                    gimli::Range { begin, end }
                } else {
                    ranges.next()?.unwrap()
                };
                writeln!(
                    w,
                    "<start-end \
                     low-off: 0x{:08x} addr 0x{:08x} \
                     high-off: 0x{:08x} addr 0x{:08x}>",
                    begin, range.begin, end, range.end
                )?;
            }
            gimli::RawRngListEntry::StartLength { begin, length } => {
                let range = ranges.next()?.unwrap();
                writeln!(
                    w,
                    "<start-length \
                     low-off: 0x{:08x} addr 0x{:08x} \
                     high-off: 0x{:08x} addr 0x{:08x}>",
                    begin, range.begin, length, range.end
                )?;
            }
        };
    }
    Ok(())
}

fn dump_line<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    dwarf: &gimli::Dwarf<R>,
) -> Result<()> {
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        writeln!(
            w,
            "\n.debug_line: line number info for unit at .debug_info offset 0x{:08x}",
            header.offset().as_debug_info_offset().unwrap().0
        )?;
        let unit = match dwarf.unit(header) {
            Ok(unit) => unit,
            Err(err) => {
                writeln_error(
                    w,
                    dwarf,
                    err.into(),
                    "Failed to parse unit root entry for dump_line",
                )?;
                continue;
            }
        };
        match dump_line_program(w, &unit, dwarf) {
            Ok(_) => (),
            Err(Error::IoError) => return Err(Error::IoError),
            Err(err) => writeln_error(w, dwarf, err, "Failed to dump line program")?,
        }
    }
    Ok(())
}

fn dump_line_program<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    unit: &gimli::Unit<R>,
    dwarf: &gimli::Dwarf<R>,
) -> Result<()> {
    if let Some(program) = unit.line_program.clone() {
        {
            let header = program.header();
            writeln!(w)?;
            writeln!(
                w,
                "Offset:                             0x{:x}",
                header.offset().0
            )?;
            writeln!(
                w,
                "Length:                             {}",
                header.unit_length()
            )?;
            writeln!(
                w,
                "DWARF version:                      {}",
                header.version()
            )?;
            writeln!(
                w,
                "Address size:                       {}",
                header.address_size()
            )?;
            writeln!(
                w,
                "Prologue length:                    {}",
                header.header_length()
            )?;
            writeln!(
                w,
                "Minimum instruction length:         {}",
                header.minimum_instruction_length()
            )?;
            writeln!(
                w,
                "Maximum operations per instruction: {}",
                header.maximum_operations_per_instruction()
            )?;
            writeln!(
                w,
                "Default is_stmt:                    {}",
                header.default_is_stmt()
            )?;
            writeln!(
                w,
                "Line base:                          {}",
                header.line_base()
            )?;
            writeln!(
                w,
                "Line range:                         {}",
                header.line_range()
            )?;
            writeln!(
                w,
                "Opcode base:                        {}",
                header.opcode_base()
            )?;

            writeln!(w)?;
            writeln!(w, "Opcodes:")?;
            for (i, length) in header
                .standard_opcode_lengths()
                .to_slice()?
                .iter()
                .enumerate()
            {
                writeln!(w, "  Opcode {} has {} args", i + 1, length)?;
            }

            let base = if header.version() >= 5 { 0 } else { 1 };
            writeln!(w)?;
            writeln!(w, "The Directory Table:")?;
            for (i, dir) in header.include_directories().iter().enumerate() {
                writeln!(
                    w,
                    "  {} {}",
                    base + i,
                    dwarf.attr_string(unit, dir.clone())?.to_string_lossy()?
                )?;
            }

            writeln!(w)?;
            writeln!(w, "The File Name Table")?;
            write!(w, "  Entry\tDir\tTime\tSize")?;
            if header.file_has_md5() {
                write!(w, "\tMD5\t\t\t\t")?;
            }
            writeln!(w, "\tName")?;
            for (i, file) in header.file_names().iter().enumerate() {
                write!(
                    w,
                    "  {}\t{}\t{}\t{}",
                    base + i,
                    file.directory_index(),
                    file.timestamp(),
                    file.size(),
                )?;
                if header.file_has_md5() {
                    let md5 = file.md5();
                    write!(w, "\t")?;
                    for i in 0..16 {
                        write!(w, "{:02X}", md5[i])?;
                    }
                }
                writeln!(
                    w,
                    "\t{}",
                    dwarf
                        .attr_string(unit, file.path_name())?
                        .to_string_lossy()?
                )?;
            }

            writeln!(w)?;
            writeln!(w, "Line Number Instructions:")?;
            let mut instructions = header.instructions();
            while let Some(instruction) = instructions.next_instruction(header)? {
                writeln!(w, "  {}", instruction)?;
            }

            writeln!(w)?;
            writeln!(w, "Line Number Rows:")?;
            writeln!(w, "<pc>        [lno,col]")?;
        }
        let mut rows = program.rows();
        let mut file_index = std::u64::MAX;
        while let Some((header, row)) = rows.next_row()? {
            let line = match row.line() {
                Some(line) => line.get(),
                None => 0,
            };
            let column = match row.column() {
                gimli::ColumnType::Column(column) => column.get(),
                gimli::ColumnType::LeftEdge => 0,
            };
            write!(w, "0x{:08x}  [{:4},{:2}]", row.address(), line, column)?;
            if row.is_stmt() {
                write!(w, " NS")?;
            }
            if row.basic_block() {
                write!(w, " BB")?;
            }
            if row.end_sequence() {
                write!(w, " ET")?;
            }
            if row.prologue_end() {
                write!(w, " PE")?;
            }
            if row.epilogue_begin() {
                write!(w, " EB")?;
            }
            if row.isa() != 0 {
                write!(w, " IS={}", row.isa())?;
            }
            if row.discriminator() != 0 {
                write!(w, " DI={}", row.discriminator())?;
            }
            if file_index != row.file_index() {
                file_index = row.file_index();
                if let Some(file) = row.file(header) {
                    if let Some(directory) = file.directory(header) {
                        write!(
                            w,
                            " uri: \"{}/{}\"",
                            dwarf.attr_string(unit, directory)?.to_string_lossy()?,
                            dwarf
                                .attr_string(unit, file.path_name())?
                                .to_string_lossy()?
                        )?;
                    } else {
                        write!(
                            w,
                            " uri: \"{}\"",
                            dwarf
                                .attr_string(unit, file.path_name())?
                                .to_string_lossy()?
                        )?;
                    }
                }
            }
            writeln!(w)?;
        }
    }
    Ok(())
}

fn dump_pubnames<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    debug_pubnames: &gimli::DebugPubNames<R>,
    debug_info: &gimli::DebugInfo<R>,
) -> Result<()> {
    writeln!(w, "\n.debug_pubnames")?;

    let mut cu_offset;
    let mut cu_die_offset = gimli::DebugInfoOffset(0);
    let mut prev_cu_offset = None;
    let mut pubnames = debug_pubnames.items();
    while let Some(pubname) = pubnames.next()? {
        cu_offset = pubname.unit_header_offset();
        if Some(cu_offset) != prev_cu_offset {
            let cu = debug_info.header_from_offset(cu_offset)?;
            cu_die_offset = gimli::DebugInfoOffset(cu_offset.0 + cu.header_size());
            prev_cu_offset = Some(cu_offset);
        }
        let die_in_cu = pubname.die_offset();
        let die_in_sect = cu_offset.0 + die_in_cu.0;
        writeln!(w,
            "global die-in-sect 0x{:08x}, cu-in-sect 0x{:08x}, die-in-cu 0x{:08x}, cu-header-in-sect 0x{:08x} '{}'",
            die_in_sect,
            cu_die_offset.0,
            die_in_cu.0,
            cu_offset.0,
            pubname.name().to_string_lossy()?
        )?;
    }
    Ok(())
}

fn dump_pubtypes<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    debug_pubtypes: &gimli::DebugPubTypes<R>,
    debug_info: &gimli::DebugInfo<R>,
) -> Result<()> {
    writeln!(w, "\n.debug_pubtypes")?;

    let mut cu_offset;
    let mut cu_die_offset = gimli::DebugInfoOffset(0);
    let mut prev_cu_offset = None;
    let mut pubtypes = debug_pubtypes.items();
    while let Some(pubtype) = pubtypes.next()? {
        cu_offset = pubtype.unit_header_offset();
        if Some(cu_offset) != prev_cu_offset {
            let cu = debug_info.header_from_offset(cu_offset)?;
            cu_die_offset = gimli::DebugInfoOffset(cu_offset.0 + cu.header_size());
            prev_cu_offset = Some(cu_offset);
        }
        let die_in_cu = pubtype.die_offset();
        let die_in_sect = cu_offset.0 + die_in_cu.0;
        writeln!(w,
            "pubtype die-in-sect 0x{:08x}, cu-in-sect 0x{:08x}, die-in-cu 0x{:08x}, cu-header-in-sect 0x{:08x} '{}'",
            die_in_sect,
            cu_die_offset.0,
            die_in_cu.0,
            cu_offset.0,
            pubtype.name().to_string_lossy()?
        )?;
    }
    Ok(())
}

fn dump_aranges<R: Reader<Offset = usize>, W: Write>(
    w: &mut W,
    debug_aranges: &gimli::DebugAranges<R>,
) -> Result<()> {
    writeln!(w, "\n.debug_aranges")?;

    let mut headers = debug_aranges.headers();
    while let Some(header) = headers.next()? {
        writeln!(
            w,
            "Address Range Header: length = 0x{:08x}, version = 0x{:04x}, cu_offset = 0x{:08x}, addr_size = 0x{:02x}, seg_size = 0x{:02x}",
            header.length(),
            header.encoding().version,
            header.debug_info_offset().0,
            header.encoding().address_size,
            header.segment_size(),
        )?;
        let mut aranges = header.entries();
        while let Some(arange) = aranges.next()? {
            let range = arange.range();
            if let Some(segment) = arange.segment() {
                writeln!(
                    w,
                    "[0x{:016x},  0x{:016x}) segment 0x{:x}",
                    range.begin, range.end, segment
                )?;
            } else {
                writeln!(w, "[0x{:016x},  0x{:016x})", range.begin, range.end)?;
            }
        }
    }
    Ok(())
}
