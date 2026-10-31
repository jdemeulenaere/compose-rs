//! A font for exact expectations, as Compose's `sample_font` (`FontTestData.BASIC_MEASURE_FONT`):
//! its metrics and the characters it has are Compose's, and tests expect them on every stack. It's
//! made here, as a TrueType file in memory, so the repository holds no font file (D1124).

use std::sync::OnceLock;

use compose_ui::{Font, FontOptions};

const UNITS_PER_EM: u16 = 1000;
/// A line is one em: these are what a paragraph's lines are made of.
const ASCENT: i16 = 800;
const DESCENT: i16 = -200;
/// What Compose's has beyond them, which a stack reading these for its lines would show: its
/// `FontTestData` calls 1.2 em the difference of a paint's top and bottom.
const TYPOGRAPHIC_LINE_GAP: i16 = 200;
const WINDOWS_ASCENT: u16 = 1000;
const WINDOWS_DESCENT: u16 = 200;

const ONE_EM: u16 = 1;
const ONE_EM_RIGHT_TO_LEFT: u16 = 2;
const SPACE: u16 = 3;
const ELLIPSIS: u16 = 4;
const LINE_FEED: u16 = 5;

/// A glyph: its advance and its one contour, if it has one.
struct Glyph {
    advance: u16,
    contour: &'static [(i16, i16)],
}

const GLYPHS: [Glyph; 6] = [
    // .notdef: what a stack falls back past.
    Glyph { advance: 500, contour: &[] },
    // Points along the text, which tells the two directions apart in a picture.
    Glyph { advance: 1000, contour: &[(0, DESCENT), (0, ASCENT), (1000, 300)] },
    Glyph { advance: 1000, contour: &[(1000, DESCENT), (0, 300), (1000, ASCENT)] },
    // A hairline, on the baseline.
    Glyph { advance: 1000, contour: &[(0, 0), (0, 10), (1000, 10), (1000, 0)] },
    // A bar below the baseline.
    Glyph { advance: 1000, contour: &[(0, DESCENT), (0, 0), (1000, 0), (1000, DESCENT)] },
    Glyph { advance: 500, contour: &[] },
];

/// The first and last character of each run mapped, and its glyph. Digits aren't there, as in
/// Compose's: a test of a fallback's width can use them.
const CHARACTERS: [(u32, u32, u16); 12] = [
    (0x09, 0x09, ONE_EM),
    (0x0A, 0x0A, LINE_FEED),
    (0x20, 0x20, SPACE),
    (0x2C, 0x2E, ONE_EM),
    (0x41, 0x5A, ONE_EM),
    (0x61, 0x7A, ONE_EM),
    (0x5D0, 0x5EA, ONE_EM_RIGHT_TO_LEFT),
    (0x2026, 0x2026, ELLIPSIS),
    (0x1D11E, 0x1D11F, ONE_EM),
    (0x1F600, 0x1F600, ONE_EM),
    (0x1F603, 0x1F604, ONE_EM),
    (0x1F606, 0x1F606, ONE_EM),
];

/// A table being written.
#[derive(Default)]
struct Table(Vec<u8>);

impl Table {
    fn u16(mut self, value: u16) -> Self {
        self.0.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn i16(self, value: i16) -> Self {
        self.u16(value as u16)
    }

    fn u32(mut self, value: u32) -> Self {
        self.0.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn zeros(mut self, count: usize) -> Self {
        self.0.resize(self.0.len() + count, 0);
        self
    }
}

const VERSION_ONE: u32 = 0x0001_0000;

fn head() -> Vec<u8> {
    Table::default()
        .u32(VERSION_ONE)
        .u32(VERSION_ONE)
        // The whole file's checksum goes here, once there is a file.
        .u32(0)
        .u32(0x5F0F_3CF5)
        // The baseline at y = 0, the left side bearing at x = 0.
        .u16(0b11)
        .u16(UNITS_PER_EM)
        // Created and modified: not a date a test could depend on.
        .zeros(16)
        .i16(0)
        .i16(DESCENT)
        .i16(1000)
        .i16(ASCENT)
        // Not bold, not italic.
        .u16(0)
        // The smallest size readable; left to right with neutrals.
        .u16(8)
        .i16(2)
        // `loca` in 32 bits; the only glyph format.
        .i16(1)
        .i16(0)
        .0
}

fn hhea() -> Vec<u8> {
    Table::default()
        .u32(VERSION_ONE)
        .i16(ASCENT)
        .i16(DESCENT)
        .i16(0)
        .u16(1000)
        // Bearings; the widest extent.
        .i16(0)
        .i16(0)
        .i16(1000)
        // A caret upright.
        .i16(1)
        .i16(0)
        .i16(0)
        .zeros(8)
        .i16(0)
        .u16(GLYPHS.len() as u16)
        .0
}

fn maxp() -> Vec<u8> {
    Table::default()
        .u32(VERSION_ONE)
        .u16(GLYPHS.len() as u16)
        .u16(4)
        .u16(1)
        // Nothing composite, no instructions: one zone, the rest zero.
        .u16(0)
        .u16(0)
        .u16(1)
        .zeros(16)
        .0
}

fn os2() -> Vec<u8> {
    let first = CHARACTERS.iter().map(|run| run.0).min().unwrap_or(0) as u16;
    Table::default()
        .u16(4)
        .i16(1000)
        // Regular, not condensed; embedding allowed.
        .u16(400)
        .u16(5)
        .u16(0)
        // Sub- and superscripts, the strikeout, a family class: nothing a test reads.
        .zeros(22)
        // Panose, Unicode ranges, the vendor: none claimed.
        .zeros(10 + 16 + 4)
        // Regular.
        .u16(0x40)
        .u16(first)
        .u16(0xFFFF)
        .i16(ASCENT)
        .i16(DESCENT)
        .i16(TYPOGRAPHIC_LINE_GAP)
        .u16(WINDOWS_ASCENT)
        .u16(WINDOWS_DESCENT)
        // Code pages; x and cap heights; default and break characters; context.
        .zeros(8)
        .i16(0)
        .i16(0)
        .u16(0)
        .u16(0x20)
        .u16(0)
        .0
}

fn hmtx() -> Vec<u8> {
    let left_bearing = |glyph: &Glyph| glyph.contour.iter().map(|point| point.0).min().unwrap_or(0);
    GLYPHS
        .iter()
        .fold(Table::default(), |table, glyph| table.u16(glyph.advance).i16(left_bearing(glyph)))
        .0
}

fn cmap() -> Vec<u8> {
    // A group per character: the glyphs of a format 12 group count up with its characters.
    let mut groups: Vec<(u32, u16)> = CHARACTERS
        .iter()
        .flat_map(|(first, last, glyph)| (*first..=*last).map(|character| (character, *glyph)))
        .collect();
    groups.sort_by_key(|group| group.0);
    let subtable_offset = 4 + 2 * 8;
    let table = Table::default()
        .u16(0)
        .u16(2)
        // Unicode's full repertoire, under the two platforms' ways of saying so.
        .u16(0)
        .u16(4)
        .u32(subtable_offset)
        .u16(3)
        .u16(10)
        .u32(subtable_offset)
        .u16(12)
        .u16(0)
        .u32(16 + groups.len() as u32 * 12)
        .u32(0)
        .u32(groups.len() as u32);
    groups
        .iter()
        .fold(table, |table, (character, glyph)| {
            table.u32(*character).u32(*character).u32(u32::from(*glyph))
        })
        .0
}

/// The outlines, and where each glyph's start.
fn glyf_and_loca() -> (Vec<u8>, Vec<u8>) {
    let mut outlines = Table::default();
    let mut offsets = Table::default().u32(0);
    let mut end = 0u32;
    for glyph in &GLYPHS {
        if !glyph.contour.is_empty() {
            let points = glyph.contour;
            let xs = points.iter().map(|point| point.0);
            let ys = points.iter().map(|point| point.1);
            let start = outlines.0.len();
            outlines = outlines
                .i16(1)
                .i16(xs.clone().min().unwrap_or(0))
                .i16(ys.clone().min().unwrap_or(0))
                .i16(xs.max().unwrap_or(0))
                .i16(ys.max().unwrap_or(0))
                // Where the contour ends; no instructions.
                .u16(points.len() as u16 - 1)
                .u16(0);
            // Every point on the curve, its coordinates in 16 bits each: deltas from the last.
            outlines.0.extend(std::iter::repeat_n(1u8, points.len()));
            let mut last = 0;
            for point in points {
                outlines = outlines.i16(point.0 - last);
                last = point.0;
            }
            last = 0;
            for point in points {
                outlines = outlines.i16(point.1 - last);
                last = point.1;
            }
            let padded = (outlines.0.len() - start).next_multiple_of(4);
            outlines.0.resize(start + padded, 0);
            end = outlines.0.len() as u32;
        }
        offsets = offsets.u32(end);
    }
    (outlines.0, offsets.0)
}

fn name() -> Vec<u8> {
    let names: [(u16, &str); 4] = [
        (1, "Compose Sample"),
        (2, "Regular"),
        (4, "Compose Sample"),
        (6, "ComposeSample-Regular"),
    ];
    let encoded: Vec<Vec<u8>> = names
        .iter()
        .map(|(_, name)| name.encode_utf16().flat_map(|unit| unit.to_be_bytes()).collect())
        .collect();
    let header = 6 + names.len() * 12;
    let mut table = Table::default().u16(0).u16(names.len() as u16).u16(header as u16);
    let mut offset = 0;
    for ((id, _), bytes) in names.iter().zip(&encoded) {
        // Windows' Unicode, American English.
        table = table.u16(3).u16(1).u16(0x409).u16(*id).u16(bytes.len() as u16).u16(offset);
        offset += bytes.len() as u16;
    }
    for bytes in &encoded {
        table.0.extend_from_slice(bytes);
    }
    table.0
}

fn post() -> Vec<u8> {
    Table::default()
        // Version 3: no glyph names. Upright; an underline; not fixed pitch by this table's word.
        .u32(0x0003_0000)
        .u32(0)
        .i16(-100)
        .i16(50)
        .zeros(20)
        .0
}

fn checksum(table: &[u8]) -> u32 {
    table.chunks(4).fold(0u32, |sum, word| {
        let mut padded = [0; 4];
        padded[..word.len()].copy_from_slice(word);
        sum.wrapping_add(u32::from_be_bytes(padded))
    })
}

fn font_file() -> Vec<u8> {
    let (glyf, loca) = glyf_and_loca();
    // In the order of their tags, as the format wants.
    let tables: [(&[u8; 4], Vec<u8>); 10] = [
        (b"OS/2", os2()),
        (b"cmap", cmap()),
        (b"glyf", glyf),
        (b"head", head()),
        (b"hhea", hhea()),
        (b"hmtx", hmtx()),
        (b"loca", loca),
        (b"maxp", maxp()),
        (b"name", name()),
        (b"post", post()),
    ];
    let count = tables.len() as u16;
    let largest_power = 1u16 << (15 - count.leading_zeros());
    let directory = Table::default()
        .u32(VERSION_ONE)
        .u16(count)
        .u16(largest_power * 16)
        .u16(largest_power.trailing_zeros() as u16)
        .u16((count - largest_power) * 16);
    let mut offset = 12 + tables.len() * 16;
    let mut head_offset = 0;
    let mut directory = directory;
    let mut contents = Vec::new();
    for (tag, table) in &tables {
        directory.0.extend_from_slice(*tag);
        directory = directory.u32(checksum(table)).u32(offset as u32).u32(table.len() as u32);
        if *tag == b"head" {
            head_offset = offset;
        }
        contents.extend_from_slice(table);
        let padded = table.len().next_multiple_of(4);
        contents.resize(contents.len() + padded - table.len(), 0);
        offset += padded;
    }
    let mut file = directory.0;
    file.extend_from_slice(&contents);
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&file));
    file[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    file
}

/// The font file of [`basic_measure_font`], for a test that needs a file or its bytes.
pub fn basic_measure_font_bytes() -> &'static [u8] {
    static FILE: OnceLock<Vec<u8>> = OnceLock::new();
    FILE.get_or_init(font_file)
}

/// Compose's `FontTestData.BASIC_MEASURE_FONT`. Every character it has but the line feed is one em
/// wide, a line is one em high with its baseline at 0.8 em, and a Hebrew letter's glyph points the
/// other way from a Latin one's. A font of bytes: a stack loads it in its `Blocking` path.
pub fn basic_measure_font() -> Font {
    Font::from_bytes(basic_measure_font_bytes(), FontOptions::default())
}
