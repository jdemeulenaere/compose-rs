//! The font compose-rs makes itself for exact expectations: is what it hands out a font file a
//! strict parser takes? What it lays out as is the measurers' tests (compose-skia's).

use compose_test::basic_measure_font_bytes;

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn checksum(table: &[u8]) -> u32 {
    table.chunks(4).fold(0u32, |sum, word| {
        let mut padded = [0; 4];
        padded[..word.len()].copy_from_slice(word);
        sum.wrapping_add(u32::from_be_bytes(padded))
    })
}

/// Each table's tag, offset and length.
fn tables(bytes: &[u8]) -> Vec<(String, usize, usize)> {
    (0..usize::from(u16_at(bytes, 4)))
        .map(|index| {
            let record = 12 + index * 16;
            let tag = String::from_utf8_lossy(&bytes[record..record + 4]).into_owned();
            (tag, u32_at(bytes, record + 8) as usize, u32_at(bytes, record + 12) as usize)
        })
        .collect()
}

fn table<'a>(bytes: &'a [u8], tag: &str) -> &'a [u8] {
    let (_, offset, length) = tables(bytes)
        .into_iter()
        .find(|(name, _, _)| name == tag)
        .unwrap_or_else(|| panic!("no {tag} table"));
    &bytes[offset..offset + length]
}

#[test]
fn the_sample_font_has_the_tables_a_font_needs_in_order() {
    let bytes = basic_measure_font_bytes();
    assert_eq!(u32_at(bytes, 0), 0x0001_0000, "TrueType outlines");
    let names: Vec<String> = tables(bytes).into_iter().map(|(name, _, _)| name).collect();
    assert_eq!(
        names,
        ["OS/2", "cmap", "glyf", "head", "hhea", "hmtx", "loca", "maxp", "name", "post"]
    );
    let count = names.len() as u16;
    let largest_power = 1u16 << (15 - count.leading_zeros());
    assert_eq!(u16_at(bytes, 6), largest_power * 16, "searchRange");
    assert_eq!(u16_at(bytes, 8), largest_power.trailing_zeros() as u16, "entrySelector");
    assert_eq!(u16_at(bytes, 10), count * 16 - largest_power * 16, "rangeShift");
}

#[test]
fn the_sample_fonts_tables_are_aligned_and_checksummed() {
    let bytes = basic_measure_font_bytes();
    for (index, (name, offset, length)) in tables(bytes).into_iter().enumerate() {
        assert_eq!(offset % 4, 0, "{name} isn't aligned");
        let stored = u32_at(bytes, 12 + index * 16 + 4);
        let mut contents = bytes[offset..offset + length].to_vec();
        if name == "head" {
            // Its checksum is taken with the adjustment at zero.
            contents[8..12].fill(0);
        }
        assert_eq!(stored, checksum(&contents), "{name}'s checksum");
    }
    // The whole file sums to the format's magic number.
    assert_eq!(checksum(bytes), 0xB1B0_AFBA);
}

#[test]
fn the_sample_fonts_metrics_are_those_compose_expects() {
    let bytes = basic_measure_font_bytes();
    let head = table(bytes, "head");
    assert_eq!(u16_at(head, 18), 1000, "units per em");
    let hhea = table(bytes, "hhea");
    assert_eq!(u16_at(hhea, 4) as i16, 800, "ascent");
    assert_eq!(u16_at(hhea, 6) as i16, -200, "descent");
    assert_eq!(u16_at(hhea, 8) as i16, 0, "line gap");
    assert_eq!(table(bytes, "maxp").len(), 32, "version 1.0's size");
    let glyphs = u16_at(table(bytes, "maxp"), 4);
    assert_eq!(u16_at(hhea, 34), glyphs, "a metric for each glyph");
    assert_eq!(table(bytes, "hmtx").len(), usize::from(glyphs) * 4);
    assert_eq!(table(bytes, "loca").len(), (usize::from(glyphs) + 1) * 4);
    let os2 = table(bytes, "OS/2");
    assert_eq!(u16_at(os2, 72) as i16, 200, "typographic line gap");
    assert_eq!(u16_at(os2, 74), 1000, "windows ascent");
    assert_eq!(u16_at(os2, 76), 200, "windows descent");
}

#[test]
fn the_sample_font_is_the_same_bytes_each_time() {
    assert!(std::ptr::eq(basic_measure_font_bytes(), basic_measure_font_bytes()));
}
