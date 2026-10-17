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

#[derive(Debug, Clone)]
pub struct Layer {
    /// Image center relative to the entity's feet.
    pub offset: [i32; 2],
    /// -1 for an empty layer.
    pub image: i32,
    pub mirror: bool,
    pub color: [u8; 4],
    pub scale: [f32; 2],
    pub angle: i32,
    /// 0 palette image, 1 RGBA image.
    pub kind: u32,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub layers: Vec<Layer>,
    pub event: i32,
    /// Attach points: where the head sits on a body frame, and the matching point on a head frame.
    pub anchors: Vec<[i32; 2]>,
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
                    let mut layer = Layer { offset, image, mirror, color: [255; 4], scale: [1.0; 2], angle: 0, kind: 0 };
                    if version >= (2, 0) {
                        layer.color = reader.take(4)?.try_into().unwrap();
                        let x = reader.f32()?;
                        layer.scale = [x, if version >= (2, 4) { reader.f32()? } else { x }];
                        layer.angle = reader.i32()?;
                        layer.kind = reader.i32()? as u32;
                        if version >= (2, 5) {
                            reader.take(8)?; // explicit width and height
                        }
                    }
                    layers.push(layer);
                }
                let event = if version >= (2, 0) { reader.i32()? } else { -1 };
                let mut anchors = Vec::new();
                if version >= (2, 3) {
                    for _ in 0..reader.i32()?.max(0) {
                        reader.take(4)?;
                        anchors.push([reader.i32()?, reader.i32()?]);
                        reader.take(4)?;
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
}

/// Places a frame's layers, shifted by `shift` (used to attach heads to bodies).
pub fn place(spr: &Spr, frame: &Frame, shift: [i32; 2]) -> Vec<Placement> {
    frame
        .layers
        .iter()
        .filter_map(|layer| {
            let image = usize::try_from(layer.image).ok()?;
            let [width, height] = spr.size(layer.kind, image)?;
            let size = [width as f32 * layer.scale[0], height as f32 * layer.scale[1]];
            // Offsets are image centers; the client rounds the top-left to whole pixels.
            let left = (layer.offset[0] + shift[0]) as f32 - (size[0] / 2.0).floor();
            let top = (layer.offset[1] + shift[1]) as f32 - (size[1] / 2.0).floor();
            Some(Placement {
                kind: layer.kind,
                image,
                position: [if layer.mirror { left + size[0] } else { left }, top],
                size: [if layer.mirror { -size[0] } else { size[0] }, size[1]],
                color: layer.color,
            })
        })
        .collect()
}

/// The shift that puts a head frame's anchor on the body frame's anchor.
pub fn head_shift(body: &Frame, head: &Frame) -> [i32; 2] {
    match (body.anchors.first(), head.anchors.first()) {
        (Some(body), Some(head)) => [body[0] - head[0], body[1] - head[1]],
        _ => [0, 0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grf::Grf;

    /// Needs the official data.grf, at `ROCLIENT_GRF` or `./data.grf`.
    #[test]
    fn parses_novice_body_and_head() {
        let grf = Grf::open(std::env::var("ROCLIENT_GRF").unwrap_or_else(|_| "data.grf".into())).unwrap();
        let path = "data\\sprite\\인간족\\몸통\\남\\초보자_남";
        let body = Spr::parse(&grf.read(&format!("{path}.spr")).unwrap()).unwrap();
        let body_act = Act::parse(&grf.read(&format!("{path}.act")).unwrap()).unwrap();
        let head = Spr::parse(&grf.read("data\\sprite\\인간족\\머리통\\남\\1_남.spr").unwrap()).unwrap();
        let head_act = Act::parse(&grf.read("data\\sprite\\인간족\\머리통\\남\\1_남.act").unwrap()).unwrap();

        // Player sprites have 8 directions per action.
        assert!(body_act.actions.len() >= 8 * 13, "{} actions", body_act.actions.len());
        assert_eq!(body_act.delays.len(), body_act.actions.len());

        let standing = &body_act.actions[0][0];
        let placements = place(&body, standing, [0, 0]);
        assert!(!placements.is_empty());
        let image = body.image(placements[0].kind, placements[0].image, None).unwrap();
        assert!(image.pixels().any(|pixel| pixel.0[3] == 255));
        assert!(image.pixels().any(|pixel| pixel.0[3] == 0));

        // The head attaches above the feet, where the body's anchor is.
        let shift = head_shift(standing, &head_act.actions[0][0]);
        let head_placements = place(&head, &head_act.actions[0][0], shift);
        assert!(head_placements.iter().all(|placement| placement.position[1] < placements[0].position[1] + 10.0));
    }
}
