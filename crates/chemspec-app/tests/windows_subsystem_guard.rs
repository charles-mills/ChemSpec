const WINDOWS_GUI_SUBSYSTEM: u16 = 2;
const PE_SIGNATURE_OFFSET: usize = 0x3c;
const COFF_HEADER_SIZE: usize = 20;
const OPTIONAL_HEADER_SUBSYSTEM_OFFSET: usize = 68;

fn little_endian_u16(bytes: &[u8], offset: usize) -> Result<u16, &'static str> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or("field falls outside the executable")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn little_endian_u32(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or("field falls outside the executable")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn pe_subsystem(bytes: &[u8]) -> Result<u16, &'static str> {
    if bytes.get(..2) != Some(b"MZ") {
        return Err("missing DOS executable signature");
    }

    let pe_offset = usize::try_from(little_endian_u32(bytes, PE_SIGNATURE_OFFSET)?)
        .map_err(|_| "PE header offset does not fit in usize")?;
    if bytes.get(pe_offset..pe_offset + 4) != Some(b"PE\0\0") {
        return Err("missing PE signature");
    }

    let coff_header = pe_offset + 4;
    let optional_header_size = usize::from(little_endian_u16(bytes, coff_header + 16)?);
    if optional_header_size < OPTIONAL_HEADER_SUBSYSTEM_OFFSET + 2 {
        return Err("optional header is too small to contain a subsystem");
    }

    let optional_header = coff_header + COFF_HEADER_SIZE;
    let optional_header_end = optional_header
        .checked_add(optional_header_size)
        .ok_or("optional header size overflows")?;
    if optional_header_end > bytes.len() {
        return Err("optional header falls outside the executable");
    }

    match little_endian_u16(bytes, optional_header)? {
        0x10b | 0x20b => {}
        _ => return Err("unsupported PE optional-header magic"),
    }

    little_endian_u16(bytes, optional_header + OPTIONAL_HEADER_SUBSYSTEM_OFFSET)
}

#[test]
fn reads_subsystem_from_pe32_plus_header() {
    let pe_offset = 0x80_usize;
    let optional_header = pe_offset + 4 + COFF_HEADER_SIZE;
    let optional_header_size = OPTIONAL_HEADER_SUBSYSTEM_OFFSET + 2;
    let mut executable = vec![0_u8; optional_header + optional_header_size];

    executable[..2].copy_from_slice(b"MZ");
    executable[PE_SIGNATURE_OFFSET..PE_SIGNATURE_OFFSET + 4]
        .copy_from_slice(&u32::try_from(pe_offset).unwrap().to_le_bytes());
    executable[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
    executable[pe_offset + 4 + 16..pe_offset + 4 + 18]
        .copy_from_slice(&u16::try_from(optional_header_size).unwrap().to_le_bytes());
    executable[optional_header..optional_header + 2].copy_from_slice(&0x20b_u16.to_le_bytes());
    executable[optional_header + OPTIONAL_HEADER_SUBSYSTEM_OFFSET
        ..optional_header + OPTIONAL_HEADER_SUBSYSTEM_OFFSET + 2]
        .copy_from_slice(&WINDOWS_GUI_SUBSYSTEM.to_le_bytes());

    assert_eq!(pe_subsystem(&executable), Ok(WINDOWS_GUI_SUBSYSTEM));
}

#[test]
fn rejects_non_pe_input() {
    assert_eq!(
        pe_subsystem(b"not an executable"),
        Err("missing DOS executable signature")
    );
}

#[cfg(all(target_os = "windows", not(debug_assertions)))]
#[test]
fn release_application_uses_windows_gui_subsystem() {
    let executable = std::fs::read(env!("CARGO_BIN_EXE_chemspec-app"))
        .expect("read the release application built for this integration test");
    let subsystem = pe_subsystem(&executable).expect("read the release application's PE subsystem");

    assert_eq!(
        subsystem, WINDOWS_GUI_SUBSYSTEM,
        "the packaged Windows application must not create a console window"
    );
}
