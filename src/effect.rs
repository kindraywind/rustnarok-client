//! The client's STR effect animations (data\texture\effect\*.str): layers of textured quads moved by keyframes.

use std::io;

/// One keyframe of a layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub frame: i32,
    /// 0 sets the state, 1 adds its values once per frame after the set before it.
    pub kind: u32,
    /// The quad's center in a 640x640 space centered on the effect (320, 320).
    pub position: [f32; 2],
    /// Corner offsets: x of the four corners, then y (top left, top right, bottom right, bottom left).
    pub corners: [f32; 8],
    /// Which texture (a fraction while it animates).
    pub texture: f32,
    /// How `texture` moves in an added key: 1 by `texture`, 2 by `delay` stopping at the last, 3 and 4 repeating
    /// forward and backward.
    pub texture_motion: u32,
    pub delay: f32,
    /// Degrees.
    pub angle: f32,
    /// RGBA, 0..1.
    pub color: [f32; 4],
    /// Direct3D blend factors (5 source alpha, 2 one, 6 one minus source alpha, 7 destination alpha).
    pub source_blend: u32,
    pub destination_blend: u32,
}

#[derive(Debug, Clone)]
pub struct Layer {
    /// Texture file names, relative to data\texture\effect\.
    pub textures: Vec<String>,
    pub keys: Vec<Key>,
}

#[derive(Debug, Clone)]
pub struct Str {
    pub fps: u32,
    /// Frames the effect lasts.
    pub frames: u32,
    pub layers: Vec<Layer>,
}

/// A layer's quad at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct State {
    pub texture: usize,
    pub position: [f32; 2],
    pub corners: [f32; 8],
    pub angle: f32,
    pub color: [f32; 4],
    /// Added to what's behind it rather than blended over.
    pub additive: bool,
}

const KEY_SIZE: usize = 124;

impl Str {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let invalid = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_owned());
        if bytes.get(..4) != Some(b"STRM") {
            return Err(invalid("not an STR file"));
        }
        let mut at = 4usize;
        let mut u32_next = || -> io::Result<u32> {
            let value = bytes.get(at..at + 4).ok_or_else(|| invalid("truncated STR"))?;
            at += 4;
            Ok(u32::from_le_bytes(value.try_into().unwrap()))
        };
        let version = u32_next()?;
        if version != 0x94 {
            return Err(invalid("only STR version 0x94 is supported"));
        }
        let fps = u32_next()?;
        let frames = u32_next()?;
        let layer_count = u32_next()?;
        // Display, group and type, unused.
        at += 16;
        let mut layers = Vec::with_capacity(layer_count as usize);
        for _ in 0..layer_count {
            let read_u32 =
                |at: usize| -> io::Result<u32> { Ok(u32::from_le_bytes(bytes.get(at..at + 4).ok_or_else(|| invalid("truncated STR"))?.try_into().unwrap())) };
            let texture_count = read_u32(at)? as usize;
            at += 4;
            let mut textures = Vec::with_capacity(texture_count);
            for _ in 0..texture_count {
                let name = bytes.get(at..at + 128).ok_or_else(|| invalid("truncated STR"))?;
                let end = name.iter().position(|&byte| byte == 0).unwrap_or(name.len());
                textures.push(encoding_rs::EUC_KR.decode_without_bom_handling(&name[..end]).0.into_owned());
                at += 128;
            }
            let key_count = read_u32(at)? as usize;
            at += 4;
            let mut keys = Vec::with_capacity(key_count);
            for _ in 0..key_count {
                let raw = bytes.get(at..at + KEY_SIZE).ok_or_else(|| invalid("truncated STR"))?;
                let word = |index: usize| u32::from_le_bytes(raw[index * 4..index * 4 + 4].try_into().unwrap());
                let float = |index: usize| f32::from_bits(word(index));
                keys.push(Key {
                    frame: word(0) as i32,
                    kind: word(1),
                    position: [float(2), float(3)],
                    // Words 4-11 are texture coordinates, which the client leaves at the full texture.
                    corners: std::array::from_fn(|corner| float(12 + corner)),
                    texture: float(20),
                    texture_motion: word(21),
                    delay: float(22),
                    angle: float(23) / (1024.0 / 360.0),
                    color: std::array::from_fn(|channel| float(24 + channel) / 255.0),
                    source_blend: word(28),
                    destination_blend: word(29),
                });
                at += KEY_SIZE;
            }
            layers.push(Layer { textures, keys });
        }
        Ok(Self { fps, frames, layers })
    }
}

impl Layer {
    /// The quad at `frame` (fractional), or None while the layer shows nothing: the last setting key before it,
    /// moved by the adding key right after it.
    pub fn state(&self, frame: f32) -> Option<State> {
        let (mut from, mut to) = (None, None);
        let (mut last_frame, mut last_set) = (0, 0);
        for (index, key) in self.keys.iter().enumerate() {
            if key.frame as f32 <= frame {
                match key.kind {
                    0 => from = Some(index),
                    1 => to = Some(index),
                    _ => {}
                }
            }
            last_frame = last_frame.max(key.frame);
            if key.kind == 0 {
                last_set = last_set.max(key.frame);
            }
        }
        let from_index = from?;
        if to.is_none() && (last_frame as f32) < frame {
            return None;
        }
        let start = &self.keys[from_index];
        // One, or the destination's alpha, which is always full here: added on.
        let additive = matches!(start.destination_blend, 2 | 7);
        let still = |key: &Key| State {
            texture: key.texture.max(0.0) as usize,
            position: key.position,
            corners: key.corners,
            angle: key.angle,
            color: key.color,
            additive,
        };
        let moving = to.filter(|&to_index| to_index == from_index + 1 && self.keys[to_index].frame == start.frame);
        let Some(to_index) = moving else {
            if to.is_some() && last_set <= start.frame {
                return None;
            }
            return Some(still(start));
        };
        let step = &self.keys[to_index];
        let delta = frame - start.frame as f32;
        let count = self.textures.len().max(1) as f32;
        let texture = match step.texture_motion {
            1 => start.texture + step.texture * delta,
            2 => (start.texture + step.delay * delta).min(count - 1.0),
            3 => (start.texture + step.delay * delta).rem_euclid(count),
            4 => (start.texture - step.delay * delta).rem_euclid(count),
            _ => 0.0,
        };
        Some(State {
            texture: texture.max(0.0) as usize,
            position: [start.position[0] + step.position[0] * delta, start.position[1] + step.position[1] * delta],
            corners: std::array::from_fn(|corner| start.corners[corner] + step.corners[corner] * delta),
            angle: start.angle + step.angle * delta,
            color: std::array::from_fn(|channel| start.color[channel] + step.color[channel] * delta),
            additive,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grf::Grf;

    /// Needs the official data.grf, at `ROCLIENT_GRF` or `./data.grf`.
    #[test]
    fn plays_the_level_up_angel() {
        let grf = Grf::open(std::env::var("ROCLIENT_GRF").unwrap_or_else(|_| "data.grf".into())).unwrap();
        let angel = Str::parse(&grf.read("data\\texture\\effect\\angel.str").unwrap()).unwrap();
        assert!(angel.fps > 0 && angel.frames > 10, "{} fps, {} frames", angel.fps, angel.frames);
        assert!(!angel.layers.is_empty());
        // Some layer shows something early on, and every texture it names exists.
        assert!(angel.layers.iter().any(|layer| layer.state(5.0).is_some()));
        for layer in &angel.layers {
            for texture in &layer.textures {
                assert!(grf.read(&format!("data\\texture\\effect\\{texture}")).is_ok(), "{texture}");
            }
        }
    }

    #[test]
    fn adds_keys_frame_by_frame() {
        let set = Key {
            frame: 0,
            kind: 0,
            position: [320.0, 320.0],
            corners: [-10.0, 10.0, 10.0, -10.0, -10.0, -10.0, 10.0, 10.0],
            texture: 0.0,
            texture_motion: 0,
            delay: 0.0,
            angle: 0.0,
            color: [1.0; 4],
            source_blend: 5,
            destination_blend: 2,
        };
        let add = Key { kind: 1, position: [1.0, -2.0], corners: [0.0; 8], color: [0.0, 0.0, 0.0, -0.1], ..set };
        let layer = Layer { textures: vec!["a.bmp".into()], keys: vec![set, add] };
        let state = layer.state(3.0).unwrap();
        assert_eq!(state.position, [323.0, 314.0]);
        assert!((state.color[3] - 0.7).abs() < 1e-6 && state.additive);
    }
}
