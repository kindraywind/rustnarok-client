//! The official sprite formats: SPR (palette and RGBA images) and ACT (actions made of layered frames).

use std::io;

use image::{Rgba, RgbaImage};

pub type Palette = [[u8; 4]; 256];

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let slice =
            self.bytes.get(self.position..self.position + count).ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "sprite file is truncated"))?;
        self.position += count;
        Ok(slice)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    /// Signature then version stored minor first; returns `(major, minor)`.
    fn header(&mut self, signature: &[u8; 2]) -> io::Result<(u8, u8)> {
        if self.take(2)? != signature {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "wrong sprite signature"));
        }
        let minor = self.u8()?;
        Ok((self.u8()?, minor))
    }
}

struct IndexedImage {
    width: u16,
    height: u16,
    pixels: Vec<u8>,
}

pub struct Spr {
    indexed: Vec<IndexedImage>,
    rgba: Vec<RgbaImage>,
    pub palette: Palette,
}

impl Spr {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut reader = Reader { bytes, position: 0 };
        let version = reader.header(b"SP")?;
        let indexed_count = reader.u16()?;
        let rgba_count = if version >= (1, 2) { reader.u16()? } else { 0 };

        let mut indexed = Vec::with_capacity(usize::from(indexed_count));
        for _ in 0..indexed_count {
            let (width, height) = (reader.u16()?, reader.u16()?);
            let size = usize::from(width) * usize::from(height);
            let pixels = if version >= (2, 1) {
                // Runs of index 0 are stored as `0, count`.
                let encoded_size = usize::from(reader.u16()?);
                let encoded = reader.take(encoded_size)?;
                let mut pixels = Vec::with_capacity(size);
                let mut bytes = encoded.iter();
                while let Some(&byte) = bytes.next() {
                    if byte == 0 {
                        let run = bytes.next().copied().unwrap_or(1).max(1);
                        pixels.resize(pixels.len() + usize::from(run), 0);
                    } else {
                        pixels.push(byte);
                    }
                }
                pixels.resize(size, 0);
                pixels
            } else {
                reader.take(size)?.to_vec()
            };
            indexed.push(IndexedImage { width, height, pixels });
        }

        let mut rgba = Vec::with_capacity(usize::from(rgba_count));
        for _ in 0..rgba_count {
            let (width, height) = (reader.u16()?, reader.u16()?);
            let data = reader.take(usize::from(width) * usize::from(height) * 4)?;
            // Stored bottom-up as ABGR.
            let pixels = data
                .chunks_exact(usize::from(width) * 4)
                .rev()
                .flat_map(|row| row.chunks_exact(4).flat_map(|pixel| [pixel[3], pixel[2], pixel[1], pixel[0]]))
                .collect();
            rgba.push(RgbaImage::from_raw(u32::from(width), u32::from(height), pixels).unwrap());
        }

        let palette = if version >= (1, 1) { parse_palette(reader.take(1024)?).unwrap() } else { [[0; 4]; 256] };
        Ok(Self { indexed, rgba, palette })
    }

    /// Image size for an ACT layer's `kind` (0 palette, 1 RGBA) and index.
    pub fn size(&self, kind: u32, index: usize) -> Option<[u32; 2]> {
        match kind {
            0 => self.indexed.get(index).map(|image| [u32::from(image.width), u32::from(image.height)]),
            _ => self.rgba.get(index).map(|image| [image.width(), image.height()]),
        }
    }

    /// Decodes an image; palette index 0 is transparent. `palette` replaces the sprite's own (dye palettes).
    pub fn image(&self, kind: u32, index: usize, palette: Option<&Palette>) -> Option<RgbaImage> {
        if kind != 0 {
            return self.rgba.get(index).cloned();
        }
        let image = self.indexed.get(index)?;
        let palette = palette.unwrap_or(&self.palette);
        let mut rgba = RgbaImage::new(u32::from(image.width), u32::from(image.height));
        for (pixel, &color) in rgba.pixels_mut().zip(&image.pixels) {
            if color != 0 {
                let [r, g, b, _] = palette[usize::from(color)];
                *pixel = Rgba([r, g, b, 255]);
            }
        }
        Some(rgba)
    }
}

/// A .pal file, or the palette at the end of an SPR: 256 RGBX entries.
pub fn parse_palette(bytes: &[u8]) -> Option<Palette> {
    let mut palette = [[0; 4]; 256];
    for (entry, color) in palette.iter_mut().zip(bytes.get(..1024)?.chunks_exact(4)) {
        entry.copy_from_slice(color);
    }
    Some(palette)
}

/// An .imf file (data\imf\<job>_<sex>.imf): by action and frame, whether a player's head goes behind its body. It
/// holds a priority per layer (0 the body, 1 the head), action and frame; the higher one is drawn first.
pub fn parse_imf(bytes: &[u8]) -> io::Result<Vec<Vec<bool>>> {
    let mut reader = Reader { bytes, position: 0 };
    let (_version, _checksum, last_layer) = (reader.f32()?, reader.i32()?, reader.i32()?);
    let mut layers = Vec::new();
    for _ in 0..=last_layer.clamp(0, 1) {
        let actions = reader.i32()?.max(0);
        let mut priorities = Vec::new();
        for _ in 0..actions {
            let mut frames = Vec::new();
            for _ in 0..reader.i32()?.max(0) {
                frames.push(reader.i32()?);
                // A point not needed for the order.
                reader.take(8)?;
            }
            priorities.push(frames);
        }
        layers.push(priorities);
    }
    let [body, head] = <[_; 2]>::try_from(layers).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "IMF without a head layer"))?;
    Ok(body.iter().zip(&head).map(|(body, head)| body.iter().zip(head).map(|(body, head)| head > body).collect()).collect())
}

#[derive(Debug, Clone)]
pub struct Layer {
    /// Image center relative to the entity's feet.
    pub offset: [i32; 2],
    /// -1 for an empty layer.
    pub image: i32,
    pub mirror: bool,
    pub color: [u8; 4],
    pub scale: [f32; 2],
    /// Degrees, turning the image clockwise about its middle.
    pub angle: i32,
    /// 0 palette image, 1 RGBA image.
    pub kind: u32,
    /// The image size the ACT gives itself (version 2.5 on), for centring; older ACTs use the image's.
    pub size: Option<[i32; 2]>,
    /// ACTs before 2.0 give the image's top-left, not its middle.
    pub top_left: bool,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub layers: Vec<Layer>,
    pub event: i32,
    /// Attach points (x, y and a kind): where the head sits on a body frame, and the matching point on a head frame.
    pub anchors: Vec<[i32; 3]>,
}

pub struct Act {
    pub actions: Vec<Vec<Frame>>,
    /// Per action, in units of 25ms per frame.
    pub delays: Vec<f32>,
    pub events: Vec<String>,
}

impl Act {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut reader = Reader { bytes, position: 0 };
        let version = reader.header(b"AC")?;
        let action_count = reader.u16()?;
        reader.take(10)?;

        let mut actions = Vec::with_capacity(usize::from(action_count));
        for _ in 0..action_count {
            let frame_count = reader.i32()?.max(0);
            let mut frames = Vec::with_capacity(frame_count as usize);
            for _ in 0..frame_count {
                reader.take(32)?; // two unused rectangles
                let layer_count = reader.i32()?.max(0);
                let mut layers = Vec::with_capacity(layer_count as usize);
                for _ in 0..layer_count {
                    let offset = [reader.i32()?, reader.i32()?];
                    let image = reader.i32()?;
                    let mirror = reader.i32()? != 0;
                    let mut layer =
                        Layer { offset, image, mirror, color: [255; 4], scale: [1.0; 2], angle: 0, kind: 0, size: None, top_left: version < (2, 0) };
                    if version >= (2, 0) {
                        layer.color = reader.take(4)?.try_into().unwrap();
                        let x = reader.f32()?;
                        layer.scale = [x, if version >= (2, 4) { reader.f32()? } else { x }];
                        layer.angle = reader.i32()?;
                        layer.kind = reader.i32()? as u32;
                        if version >= (2, 5) {
                            layer.size = Some([reader.i32()?, reader.i32()?]);
                        }
                    }
                    layers.push(layer);
                }
                let event = if version >= (2, 0) { reader.i32()? } else { -1 };
                let mut anchors = Vec::new();
                if version >= (2, 3) {
                    for _ in 0..reader.i32()?.max(0) {
                        reader.take(4)?;
                        anchors.push([reader.i32()?, reader.i32()?, reader.i32()?]);
                    }
                }
                frames.push(Frame { layers, event, anchors });
            }
            actions.push(frames);
        }

        let mut events = Vec::new();
        if version >= (2, 1) {
            for _ in 0..reader.i32()?.max(0) {
                let name = reader.take(40)?;
                let end = name.iter().position(|&byte| byte == 0).unwrap_or(name.len());
                events.push(encoding_rs::EUC_KR.decode_without_bom_handling(&name[..end]).0.into_owned());
            }
        }
        let delays = if version >= (2, 2) { (0..action_count).map(|_| reader.f32()).collect::<io::Result<_>>()? } else { vec![4.0; usize::from(action_count)] };
        Ok(Self { actions, delays, events })
    }
}

/// One image of a composed frame: top-left relative to the feet, and size (negative width when mirrored).
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub kind: u32,
    pub image: usize,
    pub position: [f32; 2],
    pub size: [f32; 2],
    /// RGBA tint the layer multiplies its image by (the shadow is half transparent this way).
    pub color: [u8; 4],
    /// Degrees clockwise about the image's middle.
    pub angle: i32,
}

/// Which of a frame's layers a part draws, and how its shift goes on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pick {
    /// Every layer, the shift zoomed with each (shields, garments, monsters and NPCs).
    All,
    /// One layer, the shift zoomed with it (a player's body, head and weapon).
    Layer(usize),
    /// One layer, the shift kept as it is whatever the layer's zoom (headgear).
    Unzoomed(usize),
}

/// Places a frame's layers (those `pick` takes), shifted by `shift` (used to attach heads to bodies) in the layer's own
/// zoomed pixels, or in plain pixels for `Pick::Unzoomed`.
pub fn place(spr: &Spr, frame: &Frame, shift: [i32; 2], pick: Pick) -> Vec<Placement> {
    let (only, unzoomed) = match pick {
        Pick::All => (None, false),
        Pick::Layer(index) => (Some(index), false),
        Pick::Unzoomed(index) => (Some(index), true),
    };
    frame
        .layers
        .iter()
        .enumerate()
        .filter(|&(index, _)| only.is_none_or(|only| only == index))
        .filter_map(|(_, layer)| {
            let image = usize::try_from(layer.image).ok()?;
            let [width, height] = spr.size(layer.kind, image)?;
            let [zoom_x, zoom_y] = layer.scale;
            if zoom_x == 0.0 || zoom_y == 0.0 {
                return None;
            }
            let centring = layer.size.unwrap_or([width as i32, height as i32]);
            let [shift_x, shift_y] = if unzoomed { [shift[0] as f32 / zoom_x, shift[1] as f32 / zoom_y] } else { shift.map(|value| value as f32) };
            let left = (corner(layer, 0, centring[0], zoom_x) + shift_x) * zoom_x;
            let top = (corner(layer, 1, centring[1], zoom_y) + shift_y) * zoom_y;
            let size = [width as f32 * zoom_x, height as f32 * zoom_y];
            Some(Placement {
                kind: layer.kind,
                image,
                position: [if layer.mirror { left + size[0] } else { left }, top],
                size: [if layer.mirror { -size[0] } else { size[0] }, size[1]],
                color: layer.color,
                angle: layer.angle,
            })
        })
        .collect()
}

/// A layer's image corner along one axis, in its zoomed pixels: the layer point less half the image, the odd pixel
/// going to the left or top, truncated toward zero.
fn corner(layer: &Layer, axis: usize, size: i32, zoom: f32) -> f32 {
    let offset = layer.offset[axis] as f32;
    if layer.top_left { offset } else { ((offset - ((size + 1) / 2) as f32 * zoom) / zoom).trunc() }
}

/// The shift that puts a head frame's anchor on the body frame's anchor: none unless both have one of the same kind.
pub fn head_shift(body: &Frame, head: &Frame) -> [i32; 2] {
    match (body.anchors.first(), head.anchors.first()) {
        (Some(body), Some(head)) if body[2] == head[2] => [body[0] - head[0], body[1] - head[1]],
        _ => [0, 0],
    }
}

/// A headgear's shift: the head's, plus what puts the headgear's anchor on the head's (none without both).
pub fn gear_shift(head_shift: [i32; 2], head: Option<&Frame>, gear: &Frame) -> [i32; 2] {
    match (head.and_then(|head| head.anchors.first()), gear.anchors.first()) {
        (Some(head), Some(gear)) => [head_shift[0] + head[0] - gear[0], head_shift[1] + head[1] - gear[1]],
        _ => head_shift,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grf::Grf;

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn parses_novice_body_and_head() {
        let Some(grf) = Grf::for_tests() else { return };
        let path = "data\\sprite\\인간족\\몸통\\남\\초보자_남";
        let body = Spr::parse(&grf.read(&format!("{path}.spr")).unwrap()).unwrap();
        let body_act = Act::parse(&grf.read(&format!("{path}.act")).unwrap()).unwrap();
        let head = Spr::parse(&grf.read("data\\sprite\\인간족\\머리통\\남\\1_남.spr").unwrap()).unwrap();
        let head_act = Act::parse(&grf.read("data\\sprite\\인간족\\머리통\\남\\1_남.act").unwrap()).unwrap();

        // Player sprites have 8 directions per action.
        assert!(body_act.actions.len() >= 8 * 13, "{} actions", body_act.actions.len());
        assert_eq!(body_act.delays.len(), body_act.actions.len());

        let standing = &body_act.actions[0][0];
        let placements = place(&body, standing, [0, 0], Pick::All);
        assert!(!placements.is_empty());
        let image = body.image(placements[0].kind, placements[0].image, None).unwrap();
        assert!(image.pixels().any(|pixel| pixel.0[3] == 255));
        assert!(image.pixels().any(|pixel| pixel.0[3] == 0));

        // The head attaches above the feet, where the body's anchor is.
        let shift = head_shift(standing, &head_act.actions[0][0]);
        let head_placements = place(&head, &head_act.actions[0][0], shift, Pick::All);
        assert!(head_placements.iter().all(|placement| placement.position[1] < placements[0].position[1] + 10.0));
    }

    /// A sprite with one palette image per size, for placing layers without the game data.
    fn images(sizes: &[[u16; 2]]) -> Spr {
        let indexed = sizes.iter().map(|&[width, height]| IndexedImage { width, height, pixels: vec![0; usize::from(width) * usize::from(height)] }).collect();
        Spr { indexed, rgba: Vec::new(), palette: [[0; 4]; 256] }
    }

    fn layer(offset: [i32; 2], image: i32, mirror: bool, scale: [f32; 2]) -> Layer {
        Layer { offset, image, mirror, color: [255; 4], scale, angle: 0, kind: 0, size: None, top_left: false }
    }

    #[test]
    fn centres_layers_with_the_odd_pixel_on_the_left_and_top() {
        let spr = images(&[[10, 8], [11, 9]]);
        let frame = |layers: Vec<Layer>| Frame { layers, event: -1, anchors: Vec::new() };
        let place_one = |layer: Layer, shift: [i32; 2]| place(&spr, &frame(vec![layer]), shift, Pick::All).remove(0);

        // Even sizes: half each way. Odd sizes: the extra column and row go left and up.
        assert_eq!(place_one(layer([0, 0], 0, false, [1.0; 2]), [0, 0]).position, [-5.0, -4.0]);
        assert_eq!(place_one(layer([0, 0], 1, false, [1.0; 2]), [0, 0]).position, [-6.0, -5.0]);
        assert_eq!(place_one(layer([3, -2], 1, false, [1.0; 2]), [0, 0]).position, [-3.0, -7.0]);
        // A mirrored layer covers the same span, drawn right to left.
        let mirrored = place_one(layer([0, 0], 1, true, [1.0; 2]), [0, 0]);
        assert_eq!((mirrored.position, mirrored.size), ([5.0, -5.0], [-11.0, 9.0]));
        // Zoomed: the corner is worked out in the layer's own pixels, truncated toward zero, then zoomed; so is a
        // head's shift.
        let zoomed = place_one(layer([7, 0], 1, false, [1.5, 2.0]), [2, 1]);
        assert_eq!(zoomed.position, [(((7.0 - 6.0 * 1.5) / 1.5_f32).trunc() + 2.0) * 1.5, (-5.0 + 1.0) * 2.0]);
        assert_eq!(zoomed.size, [16.5, 18.0]);
        // ACT 2.5 sizes centre the image; the image's own size is drawn.
        let sized = place_one(Layer { size: Some([20, 20]), ..layer([0, 0], 0, false, [1.0; 2]) }, [0, 0]);
        assert_eq!((sized.position, sized.size), ([-10.0, -10.0], [10.0, 8.0]));
        // Old ACTs give the top-left.
        assert_eq!(place_one(Layer { top_left: true, ..layer([4, 6], 1, false, [1.0; 2]) }, [0, 0]).position, [4.0, 6.0]);
        // A layer zoomed to nothing isn't drawn.
        assert!(place(&spr, &frame(vec![layer([0, 0], 0, false, [0.0, 1.0])]), [0, 0], Pick::All).is_empty());
    }

    #[test]
    fn draws_the_picked_layer_and_keeps_a_headgears_shift_unzoomed() {
        let spr = images(&[[10, 8], [12, 12]]);
        let frame = Frame { layers: vec![layer([0, 0], 0, false, [1.0; 2]), layer([0, 0], 1, false, [1.0; 2])], event: -1, anchors: Vec::new() };
        let images = |pick| place(&spr, &frame, [0, 0], pick).iter().map(|placement| placement.image).collect::<Vec<_>>();
        // A two-layer headgear frame draws layer 0; a head-hiding headgear layer 1; a shield every layer.
        assert_eq!((images(Pick::Unzoomed(0)), images(Pick::Unzoomed(1)), images(Pick::All)), (vec![0], vec![1], vec![0, 1]));
        // Zoomed 2: a head's shift doubles with its layer; a headgear's lands where it would unzoomed.
        let zoomed = Frame { layers: vec![layer([0, 0], 0, false, [2.0; 2])], ..frame.clone() };
        let at = |shift, pick| place(&spr, &zoomed, shift, pick)[0].position;
        let still = at([0, 0], Pick::Layer(0));
        assert_eq!(at([6, -4], Pick::Layer(0)), [still[0] + 12.0, still[1] - 8.0]);
        assert_eq!(at([6, -4], Pick::Unzoomed(0)), [still[0] + 6.0, still[1] - 4.0]);
    }

    #[test]
    fn shifts_heads_by_anchors_of_one_kind() {
        let frame = |anchors: Vec<[i32; 3]>| Frame { layers: Vec::new(), event: -1, anchors };
        let (body, head) = (frame(vec![[2, -70, 0]]), frame(vec![[-1, -10, 0]]));
        assert_eq!(head_shift(&body, &head), [3, -60]);
        assert_eq!(head_shift(&body, &frame(vec![[-1, -10, 1]])), [0, 0], "anchors of another kind");
        assert_eq!(head_shift(&body, &frame(Vec::new())), [0, 0]);
        // A headgear: the head's shift, then from the head's anchor to its own.
        assert_eq!(gear_shift([3, -60], Some(&head), &frame(vec![[1, -12, 0]])), [1, -58]);
        assert_eq!(gear_shift([3, -60], None, &frame(vec![[1, -12, 0]])), [3, -60]);
    }

    #[test]
    fn reads_which_frames_put_the_head_behind() {
        // Two layers, two actions: the body first everywhere but in action 1's second frame.
        let mut bytes = [1.01f32.to_le_bytes(), 0i32.to_le_bytes(), 1i32.to_le_bytes()].concat();
        let layers: [[&[i32]; 2]; 2] = [[&[1], &[1, 0]], [&[0], &[0, 1]]];
        for priorities in layers {
            bytes.extend(2i32.to_le_bytes());
            for frames in priorities {
                bytes.extend((frames.len() as i32).to_le_bytes());
                for &priority in frames {
                    bytes.extend([priority, 0, 0].iter().flat_map(|value| value.to_le_bytes()));
                }
            }
        }
        assert_eq!(parse_imf(&bytes).unwrap(), [vec![false], vec![false, true]]);
        assert!(parse_imf(&bytes[..bytes.len() - 4]).is_err());
    }
}
