//! Granny's compression type 1 ("Oodle0"): three runs of an LZ stream coded with an arithmetic coder and adaptive
//! escape models. The stream starts with three parameter sets of three u32, then bits read least significant first
//! from little-endian words.
//!
//! The coder keeps a 31-bit `low`/`high` and the bit-aligned code `value`. Each model is a list of slots (slot 0 is the
//! escape: "a symbol not seen yet", whose value follows as a plain number) with counts that grow by one a use and halve
//! when the total reaches 0x4000.
//!
//! ponytail: the counts are one flat list scanned linearly, where the writer of the stream keeps 16 group sums (the
//! sums are kept here too, because the cut that decides a slot's range uses them); the alphabets are at most 512
//! slots.

use std::io;

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("oodle0: {what}"))
}

/// One adaptive model: `f` the count of each slot, `symbols` the value of each slot, `sums` the 16 running group sums
/// (the last is the total), and the group layout those sums use.
struct Model {
    max_count: usize,
    count: usize,
    f: Vec<u16>,
    symbols: Vec<u16>,
    sums: [u16; 16],
    shift: u32,
    last_group: usize,
}

impl Model {
    fn new(max_count: usize) -> Self {
        let mut model = Self { max_count, count: 0, f: vec![0; max_count + 2], symbols: vec![0; max_count + 2], sums: [0; 16], shift: 15, last_group: 0 };
        model.layout(max_count + 1);
        model.add(0, 0x30003);
        model
    }

    /// The group size (a power of two) that makes the last group the smallest, for `n` slots.
    fn layout(&mut self, n: usize) {
        if n < 6 {
            (self.shift, self.last_group) = (15, 0);
            return;
        }
        let (mut best, mut best_shift) = (usize::MAX, 0);
        for shift in 0..16u32 {
            let size = 1usize << shift;
            let groups = (n + size - 1).div_euclid(size).min(16);
            let last = (n - (groups - 1) * size).max(size);
            if last < best {
                (best, best_shift) = (last, shift);
            }
            if size > n {
                break;
            }
        }
        self.shift = best_shift;
        self.last_group = 15 << best_shift;
    }

    /// Adds `delta` (two 16-bit halves: the even sums get the low one, the odd sums the high one) to the sums of the
    /// groups from `slot`'s on, and the low half to the slot's count.
    fn add(&mut self, slot: usize, delta: u32) {
        let (low, high) = (delta as u16, (delta >> 16) as u16);
        if slot >= self.last_group {
            self.sums[15] = self.sums[15].wrapping_add(low);
        } else {
            let mut group = slot >> self.shift;
            if group & 1 == 1 {
                self.sums[group] = self.sums[group].wrapping_add(low);
                group += 1;
            }
            for pair in group / 2..8 {
                self.sums[2 * pair] = self.sums[2 * pair].wrapping_add(low);
                self.sums[2 * pair + 1] = self.sums[2 * pair + 1].wrapping_add(high);
            }
        }
        self.f[slot] = self.f[slot].wrapping_add(low);
    }

    fn total(&self) -> u16 {
        self.sums[15]
    }

    /// The slot holding `target` of the total, and the sum of the counts before it. Counts the slot's use in the sums.
    fn find(&mut self, target: u16) -> Option<(usize, u16)> {
        let group = (0..15).find(|&group| target < self.sums[group]).unwrap_or(15);
        let start = if group == 0 { 0 } else { self.sums[group - 1] };
        for sum in &mut self.sums[group..] {
            *sum = sum.wrapping_add(1);
        }
        let (mut slot, mut sum) = match group {
            0 => (0, start),
            15 => (self.last_group, start),
            _ => (group << self.shift, start),
        };
        loop {
            let next = sum.wrapping_add(*self.f.get(slot)?);
            if target < next {
                return Some((slot, sum));
            }
            sum = next;
            slot += 1;
        }
    }

    fn group_of(&self, slot: usize) -> usize {
        if slot < self.last_group { slot >> self.shift } else { 15 }
    }

    /// Halves every count, drops the slots that reach 0, moves the biggest to the start of the last group, and sums again.
    fn rescale(&mut self) {
        self.layout(self.count + 1);
        let mut tmp = [0i32; 16];
        self.f[0] >>= 1;
        tmp[self.group_of(0)] = i32::from(self.f[0]);
        let (mut best, mut best_slot, mut slot) = (0u16, 0usize, 1usize);
        while slot <= self.count {
            let mut dropped_last = false;
            while self.f[slot] <= 1 {
                if slot >= self.count {
                    self.f[slot] = 0;
                    self.count -= 1;
                    dropped_last = true;
                    break;
                }
                self.f[slot] = self.f[self.count];
                self.f[self.count] = 0;
                self.symbols[slot] = self.symbols[self.count];
                self.count -= 1;
            }
            if dropped_last {
                break;
            }
            self.f[slot] >>= 1;
            let value = self.f[slot];
            if value > best {
                (best, best_slot) = (value, slot);
            }
            tmp[self.group_of(slot)] += i32::from(value);
            slot += 1;
        }
        if best != 0 {
            let target = if self.count < self.last_group { (self.count >> self.shift) << self.shift } else { self.last_group }.max(1);
            if best_slot != target {
                let (moved, other) = (self.f[best_slot], self.f[target]);
                self.f[target] = moved;
                tmp[self.group_of(target)] += i32::from(moved) - i32::from(other);
                tmp[self.group_of(best_slot)] += i32::from(other) - i32::from(moved);
                self.f[best_slot] = other;
                self.symbols.swap(best_slot, target);
            }
        }
        if self.count != self.max_count && self.f[0] == 0 {
            self.f[0] = 2;
            tmp[self.group_of(0)] += 2;
        }
        let mut sum = 0i32;
        for (group, part) in tmp.iter().enumerate() {
            sum = (sum + part) & 0xffff;
            self.sums[group] = sum as u16;
        }
    }
}

/// The arithmetic decoder and its bit source.
struct Coder<'a> {
    words: &'a [u8],
    next: usize,
    buffer: u32,
    left: u32,
    low: u32,
    high: u32,
    value: u32,
}

impl<'a> Coder<'a> {
    fn new(words: &'a [u8]) -> Self {
        let mut coder = Self { words, next: 0, buffer: 0, left: 0, low: 0, high: 0x7fff_ffff, value: 0 };
        let first = coder.word();
        // The first 31 bits, the first one the most significant; the 32nd is the next bit.
        coder.value = (first & 0x7fff_ffff).reverse_bits() >> 1;
        coder.buffer = first >> 31;
        coder.left = 1;
        coder
    }

    fn word(&mut self) -> u32 {
        let word = self.words.get(self.next..self.next + 4).map_or(0, |bytes| u32::from_le_bytes(bytes.try_into().unwrap()));
        self.next += 4;
        word
    }

    fn bit(&mut self) -> u32 {
        if self.left == 0 {
            let word = self.word();
            self.buffer = word >> 1;
            self.left = 31;
            return word & 1;
        }
        let bit = self.buffer & 1;
        self.buffer >>= 1;
        self.left -= 1;
        bit
    }

    /// Narrows the range to `start` … `start + freq` of `total` and takes in the bits that are settled.
    fn narrow(&mut self, start: u32, freq: u32, total: u32) -> Option<()> {
        if total == 0 {
            return None;
        }
        let range = u64::from(self.high - self.low) + 1;
        let high = (u64::from(self.low) + range * u64::from(start + freq) / u64::from(total)).wrapping_sub(1) as u32;
        let low = (u64::from(self.low) + range * u64::from(start) / u64::from(total)) as u32;
        let (mut low, mut high) = (low, high);
        while (low ^ high) & 0x4000_0000 == 0 {
            low <<= 1;
            high = high << 1 | 1;
            self.value = self.value << 1 | self.bit();
        }
        while low & 0x2000_0000 != 0 && high & 0x2000_0000 == 0 {
            low = (low & 0x1fff_ffff) << 1;
            high = high << 1 | 0x4000_0001;
            self.value = (self.value ^ 0x2000_0000) << 1 | self.bit();
        }
        (self.low, self.high, self.value) = (low & 0x7fff_ffff, high & 0x7fff_ffff, self.value & 0x7fff_ffff);
        Some(())
    }

    /// Where `value` falls among `total`.
    fn target(&self, total: u32) -> Option<u64> {
        let range = u64::from(self.high.checked_sub(self.low)?) + 1;
        let offset = u64::from(self.value.checked_sub(self.low)?) + 1;
        (u64::from(total) * offset).checked_sub(1).map(|top| top / range)
    }

    /// A number below `n`, every one equally likely.
    fn uniform(&mut self, n: u32) -> Option<u32> {
        let symbol = u32::try_from(self.target(n)?).ok()?;
        self.narrow(symbol, 1, n)?;
        Some(symbol)
    }

    /// A symbol of `model`, or a new one: `Err(slot)` is the slot a new symbol's value goes in.
    fn symbol(&mut self, model: &mut Model) -> Option<Result<u16, usize>> {
        if model.total() >= 0x4000 {
            model.rescale();
        }
        let target = self.target(u32::from(model.total()))? as u16;
        let (slot, start) = model.find(target)?;
        let freq = model.f[slot];
        self.narrow(u32::from(start), u32::from(freq), u32::from(model.total()).wrapping_sub(1) & 0xffff)?;
        model.f[slot] = model.f[slot].wrapping_add(1);
        if slot != 0 {
            return Some(Ok(model.symbols[slot]));
        }
        model.count += 1;
        if model.count > model.max_count {
            return None;
        }
        model.add(model.count, 0x20002);
        if model.count == model.max_count {
            let f0 = model.f[0];
            model.add(0, u32::from(f0.wrapping_neg()) << 16 | u32::from(f0.wrapping_neg()));
        }
        Some(Err(model.count))
    }

    /// A symbol of `model`, a new one being `n` possible values.
    fn read(&mut self, model: &mut Model, n: u32) -> Option<u16> {
        match self.symbol(model)? {
            Ok(symbol) => Some(symbol),
            Err(slot) => {
                let symbol = self.uniform(n)? as u16;
                model.symbols[slot] = symbol;
                Some(symbol)
            }
        }
    }
}

/// Match lengths of the length classes above 60.
const LONG_MATCHES: [usize; 4] = [128, 192, 256, 512];

/// Unpacks `data` (the three parameter sets, then the coded bits) into `stops[2]` bytes; the three runs end at the
/// stops.
pub fn decompress(data: &[u8], stops: [usize; 3]) -> io::Result<Vec<u8>> {
    if data.len() < 40 || stops[0] > stops[1] || stops[1] > stops[2] {
        return Err(bad("short stream"));
    }
    let mut out: Vec<u8> = Vec::with_capacity(stops[2] + 512);
    let mut coder = Coder::new(&data[36..]);
    for (set, &stop) in stops.iter().enumerate() {
        let word = |index: usize| u32::from_le_bytes(data[set * 12 + index * 4..set * 12 + index * 4 + 4].try_into().unwrap());
        let (p0, p1, p2) = (word(0), word(1), word(2));
        let (literals, distance) = (p0 & 0x1ff, (p0 >> 9) as usize);
        let low_range = distance.min(4) as u32;
        let mut literal_model = Model::new((p1 & 0x1ff) as usize);
        let mut length_models: Vec<Model> = (0..65).map(|class| Model::new(((p2 >> (24 - 8 * (class / 16).min(3))) & 0xff) as usize)).collect();
        let mut low_model = Model::new(low_range as usize);
        let mut high_model = Model::new((p1 >> 9) as usize);
        let (mut class, mut produced) = (0usize, 0usize);
        while out.len() < stop {
            let length_class = coder.read(&mut length_models[class], 65).ok_or_else(|| bad("length class"))? as usize;
            class = length_class;
            if length_class == 0 {
                let byte = coder.read(&mut literal_model, literals).ok_or_else(|| bad("literal"))?;
                out.push(byte as u8);
                produced += 1;
                continue;
            }
            if length_class > 64 {
                return Err(bad("length class"));
            }
            let length = if length_class < 0x3d { length_class + 1 } else { LONG_MATCHES[length_class - 0x3d] };
            let low = coder.read(&mut low_model, low_range).ok_or_else(|| bad("distance"))? as usize;
            let high = coder.read(&mut high_model, (distance.min(produced) >> 2) as u32 + 1).ok_or_else(|| bad("distance"))? as usize;
            produced += length;
            let back = 4 * high + low + 1;
            if back > out.len() {
                return Err(bad("match before the start"));
            }
            for _ in 0..length {
                out.push(out[out.len() - back]);
            }
        }
    }
    if out.len() != stops[2] {
        return Err(bad("run past the end"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The encoding half of the same arithmetic code, to round-trip with: standard carry-less coding with pending bits.
    struct Encoder {
        low: u32,
        high: u32,
        pending: u32,
        bits: Vec<bool>,
    }

    impl Encoder {
        fn new() -> Self {
            Self { low: 0, high: 0x7fff_ffff, pending: 0, bits: Vec::new() }
        }
        fn emit(&mut self, bit: bool) {
            self.bits.push(bit);
            for _ in 0..self.pending {
                self.bits.push(!bit);
            }
            self.pending = 0;
        }
        fn encode(&mut self, start: u32, freq: u32, total: u32) {
            let range = u64::from(self.high - self.low) + 1;
            let high = (u64::from(self.low) + range * u64::from(start + freq) / u64::from(total) - 1) as u32;
            let low = (u64::from(self.low) + range * u64::from(start) / u64::from(total)) as u32;
            (self.low, self.high) = (low, high);
            loop {
                if (self.low ^ self.high) & 0x4000_0000 == 0 {
                    self.emit(self.low & 0x4000_0000 != 0);
                    self.low = (self.low << 1) & 0x7fff_ffff;
                    self.high = ((self.high << 1) | 1) & 0x7fff_ffff;
                } else if self.low & 0x2000_0000 != 0 && self.high & 0x2000_0000 == 0 {
                    self.pending += 1;
                    self.low = (self.low & 0x1fff_ffff) << 1;
                    self.high = ((self.high << 1) | 0x4000_0001) & 0x7fff_ffff;
                } else {
                    break;
                }
            }
        }
        /// The bits as the little-endian words the decoder reads, ending with enough bits to settle the last range.
        fn finish(mut self) -> Vec<u8> {
            self.pending += 1;
            let bit = self.low & 0x2000_0000 != 0;
            self.emit(bit);
            self.bits.extend(std::iter::repeat_n(false, 96));
            let mut bytes = Vec::new();
            for chunk in self.bits.chunks(32) {
                let word = chunk.iter().enumerate().fold(0u32, |word, (index, &bit)| word | u32::from(bit) << index);
                bytes.extend(word.to_le_bytes());
            }
            bytes
        }
        fn uniform(&mut self, symbol: u32, n: u32) {
            self.encode(symbol, 1, n);
        }
        /// Mirrors `Coder::symbol`/`read`.
        fn symbol(&mut self, model: &mut Model, value: u16, n: u32) {
            if model.total() >= 0x4000 {
                model.rescale();
            }
            let known = (1..=model.count).find(|&slot| model.symbols[slot] == value && model.f[slot] != 0);
            let slot = known.unwrap_or(0);
            let start: u32 = model.f[..slot].iter().map(|&f| u32::from(f)).sum();
            let (found, from) = model.find(start as u16).unwrap();
            assert_eq!((found, u32::from(from)), (slot, start));
            let freq = u32::from(model.f[slot]);
            self.encode(start, freq, u32::from(model.total()) - 1);
            model.f[slot] += 1;
            if slot == 0 {
                model.count += 1;
                model.add(model.count, 0x20002);
                if model.count == model.max_count {
                    let f0 = model.f[0];
                    model.add(0, u32::from(f0.wrapping_neg()) << 16 | u32::from(f0.wrapping_neg()));
                }
                model.symbols[model.count] = value;
                self.uniform(u32::from(value), n);
            }
        }
    }

    /// Codes literals only: every length class is 0.
    fn literal_stream(bytes: &[u8], literal_slots: u16) -> Vec<u8> {
        let (mut classes, mut literals) = (Model::new(8), Model::new(literal_slots as usize));
        let mut encoder = Encoder::new();
        for &byte in bytes {
            encoder.symbol(&mut classes, 0, 65);
            encoder.symbol(&mut literals, u16::from(byte), 256);
        }
        let mut stream = Vec::new();
        for _ in 0..3 {
            // Literals 256, a distance range of 0; the literal model's slots; no use for the match models.
            stream.extend((256u32).to_le_bytes());
            stream.extend(u32::from(literal_slots).to_le_bytes());
            stream.extend(0x0808_0808u32.to_le_bytes());
        }
        stream.extend(encoder.finish());
        stream
    }

    #[test]
    fn decodes_what_the_matching_encoder_wrote() {
        // 20 distinct bytes in 3000 literals, so the models escape, grow and rescale (the total passes 0x4000).
        let text: Vec<u8> = (0..3000u32).map(|n| ((n * n + n / 7) % 20) as u8 * 11).collect();
        let packed = literal_stream(&text, 32);
        // All three runs share the one stream: the first holds everything, the others nothing more.
        assert_eq!(decompress(&packed, [text.len(); 3]).unwrap(), text);
    }

    #[test]
    fn a_full_alphabet_drops_its_escape() {
        let text: Vec<u8> = (0..200u32).map(|n| (n % 5) as u8).collect();
        let packed = literal_stream(&text, 5);
        assert_eq!(decompress(&packed, [text.len(); 3]).unwrap(), text);
    }

    #[test]
    fn refuses_a_short_stream() {
        assert!(decompress(&[0; 20], [4, 4, 4]).is_err());
    }
}
