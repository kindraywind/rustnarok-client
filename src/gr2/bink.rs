//! Granny's texture encoding 3: a wavelet image coder. The picture is cut into planes (luma, two colour differences and
//! optionally alpha), each a 4-level 9/7 wavelet whose bands are coded with the same arithmetic coder and adaptive models
//! as the compressed sections (`oodle0`). Images of 256 pixels or fewer are stored as plain RGBA bytes.
//!
//! A plane is `[bytes of coded symbols: u32][bytes of raw bits: u32]` then the two streams. The symbol stream (an
//! arithmetic coder) holds magnitudes; the raw stream (least significant bit first from little-endian words) holds
//! parameters, signs and the values of the models' first-seen symbols.

use std::io;

use super::oodle0::{Coder, Model};

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("texture: {what}"))
}

/// A decoded picture, 8-bit RGBA.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// The raw bit stream.
struct Bits<'a> {
    data: &'a [u8],
    next: usize,
    buffer: u32,
    left: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, next: 0, buffer: 0, left: 0 }
    }

    fn word(&mut self) -> u32 {
        let word = self.data.get(self.next..self.next + 4).map_or(0, |bytes| u32::from_le_bytes(bytes.try_into().unwrap()));
        self.next += 4;
        word
    }

    /// The next `n` (at most 16) bits.
    fn read(&mut self, n: u32) -> u32 {
        let mask = (1u32 << n) - 1;
        if self.left >= n {
            let value = self.buffer & mask;
            self.buffer >>= n;
            self.left -= n;
            return value;
        }
        let word = self.word();
        let value = (word << self.left | self.buffer) & mask;
        self.buffer = word >> (n - self.left);
        self.left += 32 - n;
        value
    }
}

/// How many bits a number takes, at most 15: the context of a coefficient.
fn bucket(value: u32) -> usize {
    ((32 - value.leading_zeros()) as usize).min(15)
}

/// A symbol of `model`: a first-seen one is a plain number, `raw_bits` bits from the raw stream or, with none, a number
/// below `n` from the coder.
fn read(model: &mut Model, coder: &mut Coder, bits: &mut Bits, n: u32, raw_bits: u32) -> io::Result<u16> {
    match coder.symbol(model).ok_or_else(|| bad("symbol"))? {
        Ok(symbol) => Ok(symbol),
        Err(slot) => {
            let symbol = if raw_bits > 0 { bits.read(raw_bits) } else { coder.uniform(n).ok_or_else(|| bad("symbol"))? } as u16;
            model.symbols[slot] = symbol;
            Ok(symbol)
        }
    }
}

/// A coded value with its sign from the raw stream, times `scale`.
fn signed(magnitude: u16, bits: &mut Bits, scale: i32) -> i32 {
    let value = i32::from(magnitude) * scale;
    if magnitude != 0 && bits.read(1) == 1 { -value } else { value }
}

struct Plane {
    samples: Vec<i16>,
}

impl Plane {
    fn at(&self, index: usize) -> io::Result<i32> {
        self.samples.get(index).map(|&sample| i32::from(sample)).ok_or_else(|| bad("sample outside the plane"))
    }

    fn put(&mut self, index: usize, value: i32) -> io::Result<()> {
        *self.samples.get_mut(index).ok_or_else(|| bad("sample outside the plane"))? = value as i16;
        Ok(())
    }

    fn fill(&mut self, at: usize, stride: usize, (width, height): (usize, usize), value: i32) -> io::Result<()> {
        for y in 0..height {
            for x in 0..width {
                self.put(at + y * stride + x, value)?;
            }
        }
        Ok(())
    }
}

/// The lowest band: a prediction from the left and above (or their mean), coded as steps.
fn lowest(plane: &mut Plane, coder: &mut Coder, bits: &mut Bits, at: usize, stride: usize, (width, height): (usize, usize)) -> io::Result<()> {
    if bits.read(1) == 1 {
        let value = bits.read(16) as i16;
        return plane.fill(at, stride, (width, height), i32::from(value));
    }
    let count = bits.read(16) + 1;
    let mut model = Model::new(count as usize);
    let first = bits.read(16) as i16;
    plane.put(at, i32::from(first))?;
    let mut current = i32::from(first);
    for x in 1..width {
        let step = read(&mut model, coder, bits, count, 0)?;
        current += signed(step, bits, 1);
        plane.put(at + x, current)?;
    }
    for y in 1..height {
        let row = at + y * stride;
        let above = row - stride;
        let step = read(&mut model, coder, bits, count, 0)?;
        let mut left = plane.at(above)? + signed(step, bits, 1);
        plane.put(row, left)?;
        for x in 1..width {
            let sum = plane.at(above + x)? + left;
            let predicted = (sum + i32::from(sum < 0)) >> 1;
            let step = read(&mut model, coder, bits, count, 0)?;
            left = predicted + signed(step, bits, 1);
            plane.put(row + x, left)?;
        }
    }
    Ok(())
}

/// What the next sample of a band is doing: coding values, in a run of zeros, or choosing the next runs.
enum Step {
    Runs,
    Values,
    Zeros,
    RowEnd,
    RowEndZeros,
}

/// A detail band: values coded in the context of their neighbours, in alternating runs of coded values and zeros.
fn detail(plane: &mut Plane, coder: &mut Coder, bits: &mut Bits, at: usize, stride: usize, (width, height): (usize, usize)) -> io::Result<()> {
    let scale = bits.read(16) as i32;
    if bits.read(1) == 1 {
        let value = bits.read(16) as i32;
        return plane.fill(at, stride, (width, height), value.wrapping_mul(scale));
    }
    let max = bits.read(16);
    let contexts = bucket((max as i32).wrapping_mul(scale) as u32) + 1;
    let mut models: Vec<Model> = (0..contexts).map(|_| Model::new(max as usize + 1)).collect();
    let (mut run_model, mut length_model) = (Model::new(0x40), Model::new(0x100));
    let first = coder.uniform(max + 1).ok_or_else(|| bad("first value"))? as u16;
    let first = signed(first, bits, scale);
    plane.put(at, first)?;
    let (mut out, mut above) = (at + 1, at);
    let (mut west, mut north_west, mut north) = (first, first, first);
    let (mut rows, skip) = (height, stride - width);
    let (mut remaining, mut coded, mut zeros) = (0usize, 0usize, 0usize);
    let mut step = if width == 1 {
        Step::RowEnd
    } else {
        remaining = width - 1;
        Step::Runs
    };
    let value = |models: &mut Vec<Model>, coder: &mut Coder, bits: &mut Bits, sum: i32| -> io::Result<i32> {
        let magnitude = read(&mut models[bucket((sum >> 2) as u32)], coder, bits, max + 1, 0)?;
        Ok(signed(magnitude, bits, scale))
    };
    loop {
        match step {
            Step::Runs => {
                let class = read(&mut run_model, coder, bits, 0x40, 6)? as usize;
                coded = if class < 0x3c { class } else { [128, 256, 512, 1024][class - 0x3c] };
                let length = read(&mut length_model, coder, bits, 0x100, 8)? as usize;
                zeros = match length {
                    0xfc..=0xff => [512, 1024, 2048, 3072][length - 0xfc] + 2,
                    0 => 0,
                    _ => length + 2,
                };
                // A pair of empty runs would never end.
                if coded == 0 && zeros == 0 {
                    return Err(bad("empty runs"));
                }
                step = Step::Values;
            }
            Step::Values => {
                if coded == 0 {
                    step = Step::Zeros;
                } else if remaining > 1 {
                    let north_east = plane.at(above)?;
                    let sum = north_east.abs() + north_west.abs() + north.abs() + west.abs();
                    let sample = value(&mut models, coder, bits, sum)?;
                    plane.put(out, sample)?;
                    west = sample;
                    (out, above, remaining, coded) = (out + 1, above + 1, remaining - 1, coded - 1);
                    (north_west, north) = (north, north_east);
                } else if remaining == 0 {
                    step = Step::RowEnd;
                } else {
                    let sum = (2 * west).abs() + north_west.abs() + north.abs();
                    let sample = value(&mut models, coder, bits, sum)?;
                    plane.put(out, sample)?;
                    (out, coded) = (out + 1, coded - 1);
                    step = Step::RowEnd;
                }
            }
            Step::Zeros => {
                if zeros == 0 {
                    step = Step::Runs;
                } else if zeros >= remaining {
                    zeros -= remaining;
                    for index in 0..remaining {
                        plane.put(out + index, 0)?;
                    }
                    out += remaining;
                    step = Step::RowEndZeros;
                } else {
                    remaining -= zeros;
                    for index in 0..zeros {
                        plane.put(out + index, 0)?;
                    }
                    (out, above) = (out + zeros, above + zeros);
                    (north_west, north, west, zeros) = (plane.at(above.wrapping_sub(2))?, plane.at(above - 1)?, 0, 0);
                    step = Step::Runs;
                }
            }
            Step::RowEnd | Step::RowEndZeros => {
                rows -= 1;
                if rows == 0 {
                    return Ok(());
                }
                out += skip;
                above = out - stride;
                north = plane.at(above)?;
                (remaining, above) = (width, above + 1);
                (north_west, west) = (north, north);
                step = match (step, zeros) {
                    (Step::RowEnd, _) => Step::Values,
                    (_, 0) => Step::Runs,
                    _ => Step::Zeros,
                };
            }
        }
    }
}

/// Wavelet synthesis (CDF 9/7 in 16-bit fixed point), one dimension: `low` and `high` coefficients to twice as many samples.
fn synthesize(low: &[i32], high: &[i32]) -> Vec<i32> {
    let n = low.len() as isize;
    // The low band mirrors about its first sample and past its last one's edge; the high band the other way round.
    let low_at = |i: isize| {
        low[if i < 0 {
            -i
        } else if i > n - 1 {
            2 * n - 1 - i
        } else {
            i
        } as usize]
    };
    let high_at = |i: isize| {
        high[if i < 0 {
            -i - 1
        } else if i > n - 1 {
            2 * n - 2 - i
        } else {
            i
        } as usize]
    };
    let round = |v: i32| {
        let t = v.wrapping_add((v >> 31) ^ 0x7fff);
        t.wrapping_add(if t < 0 { 0xffff } else { 0 }) >> 16
    };
    let mut out = Vec::with_capacity(low.len() * 2);
    for k in 0..n {
        let e: [i32; 4] = std::array::from_fn(|i| low_at(k - 1 + i as isize));
        let c: [i32; 5] = std::array::from_fn(|i| high_at(k - 2 + i as isize));
        let m = |a: i32, b: i32| a.wrapping_mul(b);
        let even = m(0x609d, c[1] + c[2]).wrapping_add(m(0xc9da, e[1])).wrapping_sub(m(0xa6b, e[0] + e[2])).wrapping_sub(m(0x61b, c[0] + c[3]));
        let odd = m(-0x9af, c[0] + c[4])
            .wrapping_sub(m(0x1086, e[0] + e[3]))
            .wrapping_add(m(0x1c52, c[1] + c[3]))
            .wrapping_add(m(0x6b08, e[1] + e[2]))
            .wrapping_sub(m(0xda4a, c[2]));
        out.push(i32::from(round(even) as i16));
        out.push(i32::from(round(odd) as i16));
    }
    out
}

/// The two-tap synthesis the narrow areas (under 12 samples wide, 10 high) use: sums and differences, halved and rounded.
fn haar(low: &[i32], high: &[i32]) -> Vec<i32> {
    let round = |v: i32| {
        let s = v.wrapping_add((v >> 31) ^ 1);
        s.wrapping_sub(s >> 31) >> 1
    };
    low.iter().zip(high).flat_map(|(&l, &h)| [round(2 * l + h), round(2 * l - h)].map(|value| i32::from(value as i16))).collect()
}

/// One level of the inverse transform: the bands in the top left of a `width` by `height` area, rows `stride` apart in
/// pairs `stride / 2` apart, become the picture of that area, laid out the same way.
fn inverse(plane: &mut Plane, stride: usize, width: usize, height: usize) -> io::Result<()> {
    let half = stride / 2;
    let mut rows = Vec::with_capacity(height);
    for k in 0..height {
        let base = (k / 2) * stride + (k % 2) * half;
        let mut row = Vec::with_capacity(width);
        for x in 0..width {
            row.push(plane.at(base + x)?);
        }
        rows.push(if width >= 12 { synthesize(&row[..width / 2], &row[width / 2..]) } else { haar(&row[..width / 2], &row[width / 2..]) });
    }
    for x in 0..width {
        let low: Vec<i32> = rows.iter().step_by(2).map(|row| row[x]).collect();
        let high: Vec<i32> = rows.iter().skip(1).step_by(2).map(|row| row[x]).collect();
        let column = if height >= 10 { synthesize(&low, &high) } else { haar(&low, &high) };
        for (r, pair) in column.chunks(2).enumerate() {
            plane.put(r * stride + x, pair[0])?;
            plane.put(r * stride + half + x, pair[1])?;
        }
    }
    Ok(())
}

/// Decodes the plane at `data[at..]`: its samples and where the next plane starts.
fn decode_plane(data: &[u8], at: usize, width: usize, height: usize) -> io::Result<(Plane, usize)> {
    let word = |index: usize| data.get(at + index..at + index + 4).map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) as usize);
    let (symbols, raw) = (word(0).ok_or_else(|| bad("short plane"))?, word(4).ok_or_else(|| bad("short plane"))?);
    let end = (8usize).checked_add(symbols).and_then(|n| n.checked_add(raw)).ok_or_else(|| bad("plane size"))?;
    if at.checked_add(end).is_none_or(|end| end > data.len()) {
        return Err(bad("plane past the end"));
    }
    let mut coder = Coder::new(&data[at + 8..at + 8 + symbols]);
    let mut bits = Bits::new(&data[at + 8 + symbols..at + end]);
    let mut plane = Plane { samples: vec![0; width * height] };
    let unit = (width / 16, height / 16);
    lowest(&mut plane, &mut coder, &mut bits, 0, 16 * width, unit)?;
    for level in 0..4 {
        let stride = (16 * width) >> level;
        let size = (unit.0 << level, unit.1 << level);
        for offset in [size.0, stride / 2, stride / 2 + size.0] {
            detail(&mut plane, &mut coder, &mut bits, offset, stride, size)?;
        }
    }
    for level in 0..4 {
        inverse(&mut plane, (16 * width) >> level, unit.0 << (level + 1), unit.1 << (level + 1))?;
    }
    Ok((plane, at + end))
}

/// The picture of a texture's pixel bytes (`width` by `height`, encoding 3).
pub fn decode(pixels: &[u8], width: usize, height: usize) -> io::Result<Image> {
    if width == 0 || height == 0 || width.saturating_mul(height) > 1 << 24 {
        return Err(bad("size"));
    }
    if width * height <= 256 {
        // Plain RGBA.
        let rgba = pixels.get(..width * height * 4).ok_or_else(|| bad("short pixels"))?.to_vec();
        return Ok(Image { width, height, rgba });
    }
    // Wavelet planes cover a whole number of 16 pixels each way.
    let (padded_width, padded_height) = ((width + 15) & !15, (height + 15) & !15);
    let mut at = 4;
    let mut planes = Vec::with_capacity(4);
    for index in 0..4 {
        // The fourth plane, alpha, is there only when the picture has any.
        if index == 3 && at + 8 > pixels.len() {
            break;
        }
        let (plane, next) = decode_plane(pixels, at, padded_width, padded_height)?;
        planes.push(plane);
        at = next;
    }
    if planes.len() < 3 {
        return Err(bad("planes"));
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let index = y * padded_width + x;
            let (luma, red_green, blue_green) = (i32::from(planes[0].samples[index]), i32::from(planes[1].samples[index]), i32::from(planes[2].samples[index]));
            let green = luma - (red_green + blue_green) / 4;
            let alpha = planes.get(3).map_or(255, |plane| i32::from(plane.samples[index]));
            rgba.extend([red_green + green, green, blue_green + green, alpha].map(|value| value.clamp(0, 255) as u8));
        }
    }
    Ok(Image { width, height, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Raw bits, least significant first, as little-endian words.
    #[derive(Default)]
    struct BitWriter(Vec<bool>);

    impl BitWriter {
        fn put(&mut self, value: u32, bits: u32) {
            self.0.extend((0..bits).map(|bit| value >> bit & 1 == 1));
        }
        fn bytes(&self) -> Vec<u8> {
            self.0.chunks(32).flat_map(|chunk| chunk.iter().enumerate().fold(0u32, |word, (bit, &set)| word | u32::from(set) << bit).to_le_bytes()).collect()
        }
    }

    /// A plane whose every band is a constant: the lowest band `low`, the 12 detail bands 0.
    fn flat_plane(low: u32) -> Vec<u8> {
        let mut bits = BitWriter::default();
        bits.put(1, 1);
        bits.put(low, 16);
        for _ in 0..12 {
            // Scale, "constant", the constant.
            bits.put(1, 16);
            bits.put(1, 1);
            bits.put(0, 16);
        }
        let raw = bits.bytes();
        let symbols = [0u8; 4];
        [(symbols.len() as u32).to_le_bytes().as_slice(), (raw.len() as u32).to_le_bytes().as_slice(), &symbols, &raw].concat()
    }

    #[test]
    fn a_flat_picture_is_a_flat_picture() {
        // Y = 2560 is 160 after the four levels (16 times less); the colour differences 512 and -16 become 32 and -1.
        let mut stream = vec![0u8; 4];
        for low in [2560, 512, 0xfff0, 4080] {
            stream.extend(flat_plane(low));
        }
        let image = decode(&stream, 256, 256).unwrap();
        assert_eq!((image.width, image.height, image.rgba.len()), (256, 256, 256 * 256 * 4));
        let [r, g, b, a] = [0, 1, 2, 3].map(|channel| image.rgba[channel]);
        assert_eq!(a, 255);
        // Luma 160, differences 32 and -1: green is 160 - (32 - 1) / 4 = 153, red and blue add theirs to it.
        let near = |value: u8, expected: i32| (i32::from(value) - expected).abs() <= 1;
        assert!(near(r, 185) && near(g, 153) && near(b, 152), "{r} {g} {b}");
        assert!(image.rgba.chunks(4).all(|pixel| pixel == [r, g, b, a]));
    }

    #[test]
    fn small_pictures_are_plain_rgba() {
        let pixels: Vec<u8> = (0..16 * 16 * 4).map(|n| n as u8).collect();
        let image = decode(&pixels, 16, 16).unwrap();
        assert_eq!(image.rgba, pixels);
        assert!(decode(&pixels[..100], 16, 16).is_err());
    }

    #[test]
    fn contexts_are_the_bit_length_capped() {
        assert_eq!([0, 1, 2, 3, 4, 127, 128, 255, 256, 0x3fff, 0x4000, u32::MAX].map(bucket), [0, 1, 2, 2, 3, 7, 8, 8, 9, 14, 15, 15]);
    }

    #[test]
    fn synthesis_keeps_a_flat_signal_flat() {
        // Both filters have the gain 1/sqrt(2) on a constant, half of it again through two passes.
        let flat = synthesize(&[1000; 8], &[0; 8]);
        assert!(flat.iter().all(|&v| (706..=708).contains(&v)), "{flat:?}");
        assert_eq!(haar(&[10, -10], &[0, 0]), vec![10, 10, -11, -11]);
        assert_eq!(haar(&[0], &[3]), vec![2, -2]);
    }

    #[test]
    fn refuses_streams_that_run_out() {
        assert!(decode(&[0; 12], 32, 32).is_err());
        assert!(decode(&[0; 4], 32, 32).is_err());
        assert!(decode(&[0; 4], 0, 32).is_err());
    }
}
