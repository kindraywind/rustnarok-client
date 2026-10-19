//! Map files: RSW (world: lights, water, objects), GND (ground mesh) and GAT (walkable cells).
//!
//! Field layouts follow the public format documentation.

use std::collections::{HashMap, HashSet};
use std::io;

use crate::grf::Grf;

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let slice =
            self.bytes.get(self.position..self.position + count).ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "map file is truncated"))?;
        self.position += count;
        Ok(slice)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn i16(&mut self) -> io::Result<i16> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn vector(&mut self) -> io::Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    /// A fixed-size, NUL-padded EUC-KR string.
    fn string(&mut self, length: usize) -> io::Result<String> {
        let bytes = self.take(length)?;
        let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(length);
        Ok(encoding_rs::EUC_KR.decode_without_bom_handling(&bytes[..end]).0.into_owned())
    }

    /// Signature then version stored major first; returns `(major, minor)`.
    fn header(&mut self, signature: &[u8; 4]) -> io::Result<(u8, u8)> {
        if self.take(4)? != signature {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "wrong map file signature"));
        }
        Ok((self.u8()?, self.u8()?))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Water {
    pub level: f32,
    pub kind: i32,
    pub wave_height: f32,
    pub wave_speed: f32,
    pub wave_pitch: f32,
    /// Frames between texture changes.
    pub animation_speed: u32,
}

#[derive(Debug, Clone)]
pub struct Light {
    /// Degrees.
    pub longitude: i32,
    pub latitude: i32,
    pub diffuse: [f32; 3],
    pub ambient: [f32; 3],
    pub opacity: f32,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub file: String,
    pub position: [f32; 3],
    /// Degrees.
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    /// How it plays its animation (0 none, 1 once, 2 looping) and how fast.
    pub anim_type: i32,
    pub anim_speed: f32,
    /// RSW 2.7 colour (R, G, B); white on older maps.
    pub tint: [u8; 3],
}

/// An ambient sound source (an RSW type-3 object): a wav played every `cycle` seconds from a box `width` × `height`
/// around `position`, heard out to `range`, at `volume` times the effect volume.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundSource {
    pub file: String,
    pub position: [f32; 3],
    pub volume: f32,
    pub width: i32,
    pub height: i32,
    pub range: f32,
    pub cycle: f32,
}

pub struct Rsw {
    pub ground_file: String,
    pub altitude_file: String,
    pub water: Option<Water>,
    /// RSW 2.2+ flag after the header. When set, opaque water writes depth before the rest.
    pub water_depth: bool,
    pub light: Light,
    pub models: Vec<Model>,
    pub sounds: Vec<SoundSource>,
}

impl Rsw {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut reader = Reader { bytes, position: 0 };
        let version = reader.header(b"GRSW")?;
        let build = if version >= (2, 5) { reader.i32()? } else { 0 };
        let water_depth = version >= (2, 2) && reader.u8()? == 1;
        reader.string(40)?; // ini file
        let ground_file = reader.string(40)?;
        let altitude_file = reader.string(40)?;
        if version >= (1, 4) {
            reader.string(40)?; // source file
        }

        let water = if version < (2, 6) {
            let mut water = Water::default();
            if version >= (1, 3) {
                water.level = reader.f32()?;
            }
            if version >= (1, 8) {
                water.kind = reader.i32()?;
                water.wave_height = reader.f32()?;
                water.wave_speed = reader.f32()?;
                water.wave_pitch = reader.f32()?;
            } else {
                // Older than 1.8: a flat default wave. The file has none of these.
                water.kind = 0;
                water.wave_height = 1.0;
                water.wave_speed = 2.0;
                water.wave_pitch = 50.0;
            }
            water.animation_speed = if version >= (1, 9) { reader.i32()? as u32 } else { 3 };
            Some(water)
        } else {
            None
        };

        let mut light = Light { longitude: 45, latitude: 45, diffuse: [1.0; 3], ambient: [0.3; 3], opacity: 0.5 };
        if version >= (1, 5) {
            light.longitude = reader.i32()?;
            light.latitude = reader.i32()?;
            light.diffuse = reader.vector()?;
            light.ambient = reader.vector()?;
        }
        if version >= (1, 7) {
            light.opacity = reader.f32()?;
        }
        if version >= (1, 6) {
            reader.take(16)?; // ground bounds
        }
        if version >= (2, 7) && build >= 221 {
            let count = reader.u32()? as usize;
            reader.take(count.saturating_mul(4))?;
        }

        let mut models = Vec::new();
        let mut sounds = Vec::new();
        for _ in 0..reader.i32()?.max(0) {
            match reader.i32()? {
                1 => {
                    let (name, anim_type, anim_speed) = if version >= (1, 3) {
                        let name = reader.string(40)?;
                        let (anim_type, anim_speed) = (reader.i32()?, reader.f32()?);
                        reader.i32()?; // block type
                        (name, anim_type, anim_speed)
                    } else {
                        (String::new(), 0, 0.0)
                    };
                    if version >= (2, 6) && build >= 186 {
                        reader.u8()?;
                    }
                    let tint = if version >= (2, 7) {
                        let [b, g, r, _] = reader.u32()?.to_le_bytes();
                        [r, g, b]
                    } else {
                        [255; 3]
                    };
                    let file = reader.string(80)?;
                    reader.string(80)?; // node name
                    let (position, rotation, scale) = (reader.vector()?, reader.vector()?, reader.vector()?);
                    models.push(Model { name, file, position, rotation, scale, anim_type, anim_speed, tint });
                }
                // Light sources and effects aren't drawn yet; their sizes are fixed.
                2 => drop(reader.take(80 + 12 + 12 + 4)?),
                // A sound: its name, the wav, where, how loud, its box, its range and cycle (before 2.0 none: 4 s).
                3 => {
                    reader.string(80)?;
                    let file = reader.string(80)?;
                    let position = reader.vector()?;
                    let volume = reader.f32()?;
                    let (width, height) = (reader.i32()?, reader.i32()?);
                    let range = reader.f32()?;
                    let cycle = if version >= (2, 0) { reader.f32()? } else { 4.0 };
                    sounds.push(SoundSource { file, position, volume, width, height, range, cycle });
                }
                4 => drop(reader.take(80 + 12 + 4 + 4 + 16)?),
                other => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown RSW object type {other}"))),
            }
        }
        Ok(Self { ground_file, altitude_file, water, water_depth, light, models, sounds })
    }
}

#[derive(Debug, Clone)]
pub struct Surface {
    pub u: [f32; 4],
    pub v: [f32; 4],
    /// -1 for none.
    pub texture: i16,
    pub lightmap: u16,
    /// BGRA.
    pub color: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct Cube {
    /// Corner heights (negative is up): south-west, south-east, north-west, north-east.
    pub heights: [f32; 4],
    /// Surface indices, -1 for none.
    pub top: i32,
    pub north: i32,
    pub east: i32,
}

/// One water zone. Heights are file heights: positive is down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterZone {
    pub level: f32,
    pub kind: i32,
    pub wave_height: f32,
    pub wave_speed: f32,
    pub wave_pitch: f32,
    pub animation_speed: i32,
}

/// GND 1.8+ water: a default block, how the map is split, and one zone per cell of that split.
#[derive(Clone, Debug, PartialEq)]
pub struct GndWater {
    pub split: [i32; 2],
    pub default: WaterZone,
    pub zones: Vec<WaterZone>,
}

pub struct Gnd {
    pub width: usize,
    pub height: usize,
    /// World units per cube (10 in every official map).
    pub zoom: f32,
    pub textures: Vec<String>,
    pub lightmap_size: [usize; 2],
    /// Per lightmap: width*height shadow intensities, then width*height RGB colors.
    pub lightmaps: Vec<Vec<u8>>,
    pub surfaces: Vec<Surface>,
    pub cubes: Vec<Cube>,
    /// Present from GND 1.8. Older maps take one zone from the RSW instead.
    pub water: Option<GndWater>,
}

impl Gnd {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut reader = Reader { bytes, position: 0 };
        let version = reader.header(b"GRGN")?;
        let width = reader.i32()?.max(0) as usize;
        let height = reader.i32()?.max(0) as usize;
        let zoom = reader.f32()?;
        let texture_count = reader.i32()?.max(0);
        let name_length = reader.i32()?.max(0) as usize;
        let textures = (0..texture_count).map(|_| reader.string(name_length)).collect::<io::Result<_>>()?;

        let lightmap_count = reader.i32()?.max(0);
        let lightmap_size = [reader.i32()?.max(0) as usize, reader.i32()?.max(0) as usize];
        reader.i32()?; // cells per grid
        let lightmap_bytes = lightmap_size[0] * lightmap_size[1] * 4;
        let lightmaps = (0..lightmap_count).map(|_| reader.take(lightmap_bytes).map(<[u8]>::to_vec)).collect::<io::Result<_>>()?;

        let surface_count = reader.i32()?.max(0);
        let mut surfaces = Vec::with_capacity(surface_count as usize);
        for _ in 0..surface_count {
            let u = [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?];
            let v = [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?];
            surfaces.push(Surface { u, v, texture: reader.i16()?, lightmap: reader.i16()? as u16, color: reader.take(4)?.try_into().unwrap() });
        }

        let mut cubes = Vec::with_capacity(width * height);
        for _ in 0..width * height {
            let heights = [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?];
            let mut index = || -> io::Result<i32> { if version >= (1, 7) { reader.i32() } else { reader.i16().map(i32::from) } };
            cubes.push(Cube { heights, top: index()?, north: index()?, east: index()? });
        }
        let water = (version >= (1, 8)).then(|| read_gnd_water(&mut reader, version)).transpose()?;
        Ok(Self { width, height, zoom, textures, lightmap_size, lightmaps, surfaces, cubes, water })
    }
}

fn read_zone(reader: &mut Reader) -> io::Result<WaterZone> {
    Ok(WaterZone {
        level: reader.f32()?,
        kind: reader.i32()?,
        wave_height: reader.f32()?,
        wave_speed: reader.f32()?,
        wave_pitch: reader.f32()?,
        animation_speed: reader.i32()?,
    })
}

fn read_gnd_water(reader: &mut Reader, version: (u8, u8)) -> io::Result<GndWater> {
    let default = read_zone(reader)?;
    let split = [reader.i32()?, reader.i32()?];
    let count = (split[0].max(0) as usize).saturating_mul(split[1].max(0) as usize);
    let zones = if version >= (1, 9) {
        (0..count).map(|_| read_zone(reader)).collect::<io::Result<_>>()?
    } else {
        (0..count)
            .map(|_| {
                let mut zone = default;
                zone.level = reader.f32()?;
                Ok(zone)
            })
            .collect::<io::Result<_>>()?
    };
    Ok(GndWater { split, default, zones })
}

/// The one zone an older GND takes from its RSW. Speed is truncated into 0..360; pitch is truncated.
pub fn rsw_zone(water: &Water) -> WaterZone {
    WaterZone {
        level: water.level,
        kind: water.kind,
        wave_height: water.wave_height,
        wave_speed: water.wave_speed.trunc().rem_euclid(360.0),
        wave_pitch: water.wave_pitch.trunc(),
        animation_speed: water.animation_speed as i32,
    }
}

/// Tile (i, j)'s zone, clamped into the split. A zero span is treated as one tile.
pub fn zone_index(i: usize, j: usize, [width, height]: [usize; 2], [split_w, split_h]: [usize; 2]) -> usize {
    let axis = |index: usize, tiles: usize, parts: usize| {
        let parts = parts.max(1);
        let span = (tiles / parts).max(1);
        (index / span).min(parts - 1)
    };
    let (zx, zy) = (axis(i, width, split_w), axis(j, height, split_h));
    zy * split_w.max(1) + zx
}

/// File height of corner (gx, gz). Positive is down. Wave height 0 stays at `level`.
pub fn corner_wave(zone: &WaterZone, phase: f32, gx: i32, gz: i32) -> f32 {
    let degrees = ((gx as f32 + gz as f32 - 1.0) * zone.wave_pitch + phase).trunc();
    zone.level + zone.wave_height * degrees.to_radians().sin()
}

/// A tile is under water when any top corner is lower (further down) than the water at (i + 1, j).
pub fn tile_under_water(heights: [f32; 4], water: f32) -> bool {
    heights.into_iter().any(|height| height > water)
}

/// One texture covers 4×4 tiles. Returns the south-west UV and the opposite corner.
pub fn water_uv(i: usize, j: usize) -> ([f32; 2], [f32; 2]) {
    let start = [(i % 4) as f32 / 4.0, (j % 4) as f32 / 4.0];
    (start, [start[0] + 0.25, start[1] + 0.25])
}

/// One 16 ms tick: counter, phase (360 comes off once it passes 180) and the texture frame.
pub fn step_water(counter: u32, phase: f32, animation_speed: i32, wave_speed: f32) -> (u32, f32, u32) {
    // ponytail: an animation speed of 0 would stick the frame; it advances like speed 1.
    let anim = animation_speed.max(1) as u32;
    let counter = if counter + 1 >= anim * 32 { 0 } else { counter + 1 };
    let frame = (counter / anim) % 32;
    let phase = phase + wave_speed;
    let phase = if phase > 180.0 { phase - 360.0 } else { phase };
    (counter, phase, frame)
}

pub struct Gat {
    pub width: usize,
    pub height: usize,
    /// Per cell: corner heights (south-west, south-east, north-west, north-east; negative is up) and type.
    pub cells: Vec<([f32; 4], u8)>,
}

impl Gat {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut reader = Reader { bytes, position: 0 };
        reader.header(b"GRAT")?;
        let width = reader.i32()?.max(0) as usize;
        let height = reader.i32()?.max(0) as usize;
        let cells =
            (0..width * height).map(|_| Ok(([reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?], reader.i32()? as u8))).collect::<io::Result<_>>()?;
        Ok(Self { width, height, cells })
    }

    /// Every type but 1 (a wall) and 5 (a gap that can be shot across) can be walked on, as rathena has it.
    pub fn walkable(&self, cell: [i32; 2]) -> bool {
        self.walkable_by(cell, false)
    }

    /// Whether a walker can step onto a cell: types 1 and 5 block, except type 5 for an NPC with a monster's look.
    pub fn walkable_by(&self, [x, y]: [i32; 2], monster_look: bool) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < self.width
            && (y as usize) < self.height
            && match self.cells[y as usize * self.width + x as usize].1 {
                1 => false,
                5 => monster_look,
                _ => true,
            }
    }

    /// Average height of a cell (negative is up).
    pub fn height(&self, [x, y]: [i32; 2]) -> f32 {
        let x = x.clamp(0, self.width as i32 - 1) as usize;
        let y = y.clamp(0, self.height as i32 - 1) as usize;
        self.cells[y * self.width + x].0.iter().sum::<f32>() / 4.0
    }

    /// The ground's height at a point in cells (cell (x, y) spans x..x+1, y..y+1; y north), between its cell's four
    /// corner heights.
    pub fn height_at(&self, [x, y]: [f32; 2]) -> f32 {
        let cell = [x.floor() as i32, y.floor() as i32];
        let [west, south] = [cell[0].clamp(0, self.width as i32 - 1), cell[1].clamp(0, self.height as i32 - 1)];
        let [south_west, south_east, north_west, north_east] = self.cells[south as usize * self.width + west as usize].0;
        let [across, up] = [(x - cell[0] as f32).clamp(0.0, 1.0), (y - cell[1] as f32).clamp(0.0, 1.0)];
        let south_side = south_west + (south_east - south_west) * across;
        let north_side = north_west + (north_east - north_west) * across;
        south_side + (north_side - south_side) * up
    }
}

/// Maps the client treats as indoors (closer camera, limited rotation), from data\indoorrswtable.txt.
pub fn indoor_maps(grf: &Grf) -> io::Result<HashSet<String>> {
    let bytes = grf.read("data\\indoorrswtable.txt")?;
    let table = encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0;
    Ok(table
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .map(|line| line.trim_end_matches('#').trim_end_matches(".rsw").to_lowercase())
        .collect())
}

/// Maps' display names by file name (prontera -> 룬미드가츠 왕국 수도 프론테라), from data\mapnametable.txt.
pub fn map_names(grf: &Grf) -> io::Result<HashMap<String, String>> {
    let bytes = grf.read("data\\mapnametable.txt")?;
    let table = encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0;
    Ok(table
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim().split('#');
            let file = fields.next()?.strip_suffix(".rsw")?;
            Some((file.to_lowercase(), fields.next()?.trim().to_owned()))
        })
        .collect())
}

/// A map's own camera (data\viewpointtable.txt): the distance (world units), turn and tilt (degrees above the
/// horizon) it keeps within, and where each starts on arriving.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewpoint {
    pub distance: [f32; 2],
    pub distance_in: f32,
    pub yaw: [f32; 2],
    pub yaw_in: f32,
    pub pitch: [f32; 2],
    pub pitch_in: f32,
}

/// The maps with their own camera, by map name (lowercase, no extension).
pub fn viewpoints(grf: &Grf) -> io::Result<HashMap<String, Viewpoint>> {
    let bytes = grf.read("data\\viewpointtable.txt")?;
    Ok(parse_viewpoints(&encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0))
}

/// Rows `map.rsw#range#scope#range_IN#rotationFrom#rotationTo#rotation_IN#altitudeFrom#altitudeTo#altitude_IN#`: the
/// distance from range to range + scope, altitudes as negative degrees.
pub fn parse_viewpoints(text: &str) -> HashMap<String, Viewpoint> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('#').map(str::trim).collect();
            let name = fields.first()?.trim_end_matches(".rsw").to_lowercase();
            let numbers = fields.get(1..10)?.iter().map(|field| field.parse().ok()).collect::<Option<Vec<f32>>>()?;
            let [range, scope, range_in, from, to, yaw_in, altitude_from, altitude_to, altitude_in] = <[f32; 9]>::try_from(numbers).ok()?;
            let pitch = [-altitude_from, -altitude_to];
            let viewpoint = Viewpoint {
                distance: [range, range + scope],
                distance_in: range_in,
                yaw: [from.min(to), from.max(to)],
                yaw_in,
                pitch: [pitch[0].min(pitch[1]), pitch[0].max(pitch[1])],
                pitch_in: -altitude_in,
            };
            Some((name, viewpoint))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-made RSW of `version` holding one sound object.
    fn rsw_with_a_sound(version: (u8, u8)) -> Vec<u8> {
        let text = |bytes: &mut Vec<u8>, value: &str, length: usize| {
            let mut field = value.as_bytes().to_vec();
            field.resize(length, 0);
            bytes.extend(field);
        };
        let numbers = |bytes: &mut Vec<u8>, values: &[f32]| values.iter().for_each(|value| bytes.extend(value.to_le_bytes()));
        let mut bytes = b"GRSW".to_vec();
        bytes.extend([version.0, version.1]);
        for name in ["", "prontera.gnd", "prontera.gat", ""] {
            text(&mut bytes, name, 40);
        }
        // Water: level, kind, wave height, speed, pitch, animation speed.
        numbers(&mut bytes, &[0.0]);
        bytes.extend(0i32.to_le_bytes());
        numbers(&mut bytes, &[1.0, 2.0, 50.0]);
        bytes.extend(3i32.to_le_bytes());
        // Light: longitude, latitude, diffuse, ambient, opacity; then the ground bounds.
        bytes.extend(45i32.to_le_bytes());
        bytes.extend(45i32.to_le_bytes());
        numbers(&mut bytes, &[1.0, 1.0, 1.0, 0.3, 0.3, 0.3, 0.5]);
        bytes.extend([0; 16]);
        bytes.extend(1i32.to_le_bytes());
        bytes.extend(3i32.to_le_bytes());
        text(&mut bytes, "bell", 80);
        text(&mut bytes, "se_prtchbell_03.wav", 80);
        numbers(&mut bytes, &[-12.5, -40.0, 610.0, 0.8]);
        bytes.extend(50i32.to_le_bytes());
        bytes.extend(45i32.to_le_bytes());
        numbers(&mut bytes, &[70.0]);
        if version >= (2, 0) {
            numbers(&mut bytes, &[6.0]);
        }
        bytes
    }

    #[test]
    fn reads_sound_sources() {
        let rsw = Rsw::parse(&rsw_with_a_sound((2, 1))).unwrap();
        let bell =
            SoundSource { file: "se_prtchbell_03.wav".into(), position: [-12.5, -40.0, 610.0], volume: 0.8, width: 50, height: 45, range: 70.0, cycle: 6.0 };
        assert_eq!((rsw.sounds.as_slice(), rsw.models.len()), (std::slice::from_ref(&bell), 0));
        // Before 2.0 there's no cycle: 4 seconds.
        let old = Rsw::parse(&rsw_with_a_sound((1, 9))).unwrap();
        assert_eq!(old.sounds[0].cycle, 4.0);
    }

    #[test]
    fn reads_rsw_2_7_models_and_tints() {
        let rsw_with_a_model = |version: (u8, u8), build: i32, list: &[u32]| {
            let text = |bytes: &mut Vec<u8>, value: &str, length: usize| {
                let mut field = value.as_bytes().to_vec();
                field.resize(length, 0);
                bytes.extend(field);
            };
            let numbers = |bytes: &mut Vec<u8>, values: &[f32]| values.iter().for_each(|value| bytes.extend(value.to_le_bytes()));
            let mut bytes = b"GRSW".to_vec();
            bytes.extend([version.0, version.1]);
            bytes.extend(build.to_le_bytes());
            bytes.push(1);
            for name in ["", "prt_mk.gnd", "prt_mk.gat", ""] {
                text(&mut bytes, name, 40);
            }
            bytes.extend(45i32.to_le_bytes());
            bytes.extend(45i32.to_le_bytes());
            numbers(&mut bytes, &[1.0, 1.0, 1.0, 0.3, 0.3, 0.3, 0.5]);
            bytes.extend([0; 16]);
            if version >= (2, 7) && build >= 221 {
                bytes.extend((list.len() as u32).to_le_bytes());
                list.iter().for_each(|entry| bytes.extend(entry.to_le_bytes()));
            }
            bytes.extend(1i32.to_le_bytes());
            bytes.extend(1i32.to_le_bytes());
            text(&mut bytes, "lamp", 40);
            bytes.extend(2i32.to_le_bytes());
            numbers(&mut bytes, &[1.0]);
            bytes.extend(0i32.to_le_bytes());
            if build >= 186 {
                bytes.push(0);
            }
            if version >= (2, 7) {
                bytes.extend(0xFF336699u32.to_le_bytes());
            }
            text(&mut bytes, "lamp.rsm", 80);
            text(&mut bytes, "Object01", 80);
            numbers(&mut bytes, &[12.0, -5.0, 34.0, 0.0, 90.0, 0.0, 1.0, 1.0, 1.0]);
            bytes
        };

        let rsw = Rsw::parse(&rsw_with_a_model((2, 7), 221, &[0xFF112233, 0xFF445566])).unwrap();
        assert_eq!(rsw.models.len(), 1);
        assert_eq!((rsw.models[0].file.as_str(), rsw.models[0].position, rsw.models[0].tint), ("lamp.rsm", [12.0, -5.0, 34.0], [0x33, 0x66, 0x99]));

        let empty_list = Rsw::parse(&rsw_with_a_model((2, 7), 221, &[])).unwrap();
        assert_eq!(
            (empty_list.models[0].file.as_str(), empty_list.models[0].position, empty_list.models[0].tint),
            ("lamp.rsm", [12.0, -5.0, 34.0], [0x33, 0x66, 0x99])
        );

        let build_220 = Rsw::parse(&rsw_with_a_model((2, 7), 220, &[])).unwrap();
        assert_eq!(
            (build_220.models[0].file.as_str(), build_220.models[0].position, build_220.models[0].tint),
            ("lamp.rsm", [12.0, -5.0, 34.0], [0x33, 0x66, 0x99])
        );

        let rsw_2_6 = Rsw::parse(&rsw_with_a_model((2, 6), 221, &[])).unwrap();
        assert_eq!((rsw_2_6.models[0].file.as_str(), rsw_2_6.models[0].tint), ("lamp.rsm", [255, 255, 255]));
    }

    #[test]
    fn finds_the_ground_between_a_cells_corners() {
        // One cell sloping up to the north-east (negative is up), one flat.
        let gat = Gat { width: 2, height: 1, cells: vec![([0.0, -2.0, -4.0, -6.0], 0), ([-1.0; 4], 0)] };
        assert_eq!(gat.height_at([0.0, 0.0]), 0.0);
        assert_eq!(gat.height_at([0.5, 0.5]), -3.0);
        assert_eq!(gat.height_at([0.25, 0.0]), -0.5);
        assert_eq!(gat.height_at([0.0, 0.75]), -3.0);
        assert_eq!(gat.height_at([1.5, 0.5]), -1.0);
        // Off the map: the nearest cell's edge.
        assert_eq!(gat.height_at([-3.0, 0.5]), -2.0);
    }

    #[test]
    fn walks_on_every_type_but_walls_and_gaps() {
        let gat = Gat { width: 7, height: 1, cells: (0..7).map(|kind| ([0.0; 4], kind)).collect() };
        let walkable: Vec<bool> = (0..7).map(|x| gat.walkable([x, 0])).collect();
        assert_eq!(walkable, [true, false, true, true, true, false, true]);
        assert!(!gat.walkable([7, 0]) && !gat.walkable([-1, 0]));
    }

    #[test]
    fn reads_the_viewpoint_table() {
        let text = "// range 230, scope 170\n//ex_map02.rsw#\t250#\t250#\t500#\t0#\t90#\t45#\t-40#\t-65#\t-65#\n\
                    ecl_hub01.rsw#\t210#\t50#\t250#\t\t45#\t\t65#\t\t50#\t\t-35#\t\t-45#\t\t-40#\nbroken.rsw#1#2#\n";
        let table = parse_viewpoints(text);
        assert_eq!(table.len(), 1);
        let hub = table["ecl_hub01"];
        assert_eq!(
            (hub.distance, hub.distance_in, hub.yaw, hub.yaw_in, hub.pitch, hub.pitch_in),
            ([210.0, 260.0], 250.0, [45.0, 65.0], 50.0, [35.0, 45.0], 40.0)
        );
    }

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn parses_prontera() {
        let Some(grf) = Grf::for_tests() else { return };
        let rsw = Rsw::parse(&grf.read("data\\prontera.rsw").unwrap()).unwrap();
        assert_eq!(rsw.ground_file, "prontera.gnd");
        assert_eq!(rsw.altitude_file, "prontera.gat");
        assert!(rsw.models.len() > 100, "{} models", rsw.models.len());
        assert!(rsw.models.iter().all(|model| model.file.to_lowercase().ends_with(".rsm")));

        let gnd = Gnd::parse(&grf.read("data\\prontera.gnd").unwrap()).unwrap();
        assert_eq!((gnd.width, gnd.height, gnd.zoom), (156, 196, 10.0));
        assert!(gnd.cubes.iter().all(|cube| cube.top < gnd.surfaces.len() as i32));
        assert!(gnd.surfaces.iter().all(|surface| (surface.texture as usize) < gnd.textures.len() || surface.texture < 0));
        assert_eq!(gnd.lightmap_size, [8, 8]);

        let gat = Gat::parse(&grf.read("data\\prontera.gat").unwrap()).unwrap();
        assert_eq!((gat.width, gat.height), (gnd.width * 2, gnd.height * 2));
        // The usual spawn point in front of the castle road is walkable.
        assert!(gat.walkable([156, 191]));

        let indoor = indoor_maps(&grf).unwrap();
        assert!(indoor.contains("mbase_in") && indoor.contains("prt_in") && !indoor.contains("prontera"), "{} maps", indoor.len());

        let names = map_names(&grf).unwrap();
        assert_eq!(names.get("prontera").map(String::as_str), Some("룬미드가츠 왕국 수도 프론테라"));
    }

    fn zone_bytes(level: f32, kind: i32, height: f32, speed: f32, pitch: f32, anim: i32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(level.to_le_bytes());
        bytes.extend(kind.to_le_bytes());
        bytes.extend(height.to_le_bytes());
        bytes.extend(speed.to_le_bytes());
        bytes.extend(pitch.to_le_bytes());
        bytes.extend(anim.to_le_bytes());
        bytes
    }

    /// A 1×1 GND of `version` with `water` after the cube.
    fn gnd_with_water(version: (u8, u8), water: &[u8]) -> Vec<u8> {
        let mut bytes = b"GRGN".to_vec();
        bytes.extend([version.0, version.1]);
        bytes.extend(1i32.to_le_bytes());
        bytes.extend(1i32.to_le_bytes());
        bytes.extend(10f32.to_le_bytes());
        bytes.extend(0i32.to_le_bytes());
        bytes.extend(0i32.to_le_bytes());
        bytes.extend(0i32.to_le_bytes());
        bytes.extend(8i32.to_le_bytes());
        bytes.extend(8i32.to_le_bytes());
        bytes.extend(1i32.to_le_bytes());
        bytes.extend(0i32.to_le_bytes());
        for _ in 0..4 {
            bytes.extend(0f32.to_le_bytes());
        }
        for _ in 0..3 {
            bytes.extend((-1i32).to_le_bytes());
        }
        bytes.extend(water);
        bytes
    }

    #[test]
    fn reads_gnd_1_8_and_1_9_water_zones() {
        let mut water = zone_bytes(5.0, 1, 2.0, 3.0, 4.0, 6);
        water.extend(2i32.to_le_bytes());
        water.extend(1i32.to_le_bytes());
        water.extend(9f32.to_le_bytes());
        water.extend(11f32.to_le_bytes());
        let gnd = Gnd::parse(&gnd_with_water((1, 8), &water)).unwrap();
        let zones = &gnd.water.unwrap().zones;
        assert_eq!(zones[0].level, 9.0);
        assert_eq!((zones[0].kind, zones[0].wave_speed, zones[1].level), (1, 3.0, 11.0));

        let mut full = zone_bytes(5.0, 1, 2.0, 3.0, 4.0, 6);
        full.extend(1i32.to_le_bytes());
        full.extend(1i32.to_le_bytes());
        full.extend(zone_bytes(8.0, 4, 0.0, 7.5, 9.0, 2));
        let gnd = Gnd::parse(&gnd_with_water((1, 9), &full)).unwrap();
        let zone = gnd.water.unwrap().zones[0];
        assert_eq!((zone.level, zone.kind, zone.wave_speed, zone.animation_speed), (8.0, 4, 7.5, 2));
    }

    #[test]
    fn truncates_an_old_rsw_wave_and_fills_its_defaults() {
        let mut bytes = b"GRSW".to_vec();
        bytes.extend([1, 7]);
        bytes.extend(vec![0; 160]);
        bytes.extend(3.5f32.to_le_bytes());
        bytes.extend(45i32.to_le_bytes());
        bytes.extend(30i32.to_le_bytes());
        for value in [1.0f32, 1.0, 1.0, 0.3, 0.3, 0.3, 1.0] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([0; 16]);
        bytes.extend(0i32.to_le_bytes());
        let old = Rsw::parse(&bytes).unwrap().water.unwrap();
        assert_eq!((old.level, old.kind, old.wave_height, old.wave_speed, old.wave_pitch, old.animation_speed), (3.5, 0, 1.0, 2.0, 50.0, 3));

        let mut newer = b"GRSW".to_vec();
        newer.extend([1, 9]);
        newer.extend(vec![0; 160]);
        newer.extend(zone_bytes(1.0, 0, 1.0, 362.7, -3.2, 3));
        newer.extend(45i32.to_le_bytes());
        newer.extend(45i32.to_le_bytes());
        for value in [1.0f32, 1.0, 1.0, 0.3, 0.3, 0.3, 0.5] {
            newer.extend(value.to_le_bytes());
        }
        newer.extend([0; 16]);
        newer.extend(0i32.to_le_bytes());
        let zone = rsw_zone(&Rsw::parse(&newer).unwrap().water.unwrap());
        assert_eq!((zone.wave_speed, zone.wave_pitch), (2.0, -3.0));
    }

    #[test]
    fn waves_a_corner_and_picks_tiles_zones_and_frames() {
        let zone = WaterZone { level: 10.0, kind: 0, wave_height: 1.0, wave_speed: 2.0, wave_pitch: 50.0, animation_speed: 3 };
        let crest = corner_wave(&zone, 0.0, 1, 1);
        let expected = 10.0 + 50.0_f32.to_radians().sin();
        assert!((crest - expected).abs() < 1e-5, "{crest}");
        assert_eq!(corner_wave(&zone, 0.0, 0, 1), 10.0);
        let flat = WaterZone { wave_height: 0.0, ..zone };
        assert_eq!(corner_wave(&flat, 40.0, 3, 4), 10.0);
        // (i + 1, j) water is the same formula as (i, j + 1).
        assert_eq!(corner_wave(&zone, 12.0, 4, 2), corner_wave(&zone, 12.0, 3, 3));

        assert!(tile_under_water([0.0, 0.0, 0.0, 2.0], 1.0));
        assert!(!tile_under_water([0.0; 4], 1.0));
        assert_eq!(water_uv(5, 6), ([0.25, 0.5], [0.5, 0.75]));
        assert_eq!(water_uv(0, 0).0, water_uv(4, 8).0);
        assert_eq!(zone_index(9, 0, [10, 10], [3, 1]), 2);
        assert_eq!(zone_index(3, 0, [10, 10], [3, 1]), 1);
        assert_eq!(zone_index(0, 4, [10, 8], [1, 2]), 1);

        let (counter, phase, frame) = step_water(0, 0.0, 3, 2.0);
        assert_eq!((counter, phase, frame), (1, 2.0, 0));
        let (counter, phase, frame) = (0..3).fold((0, 0.0, 0), |(counter, phase, _), _| step_water(counter, phase, 3, 2.0));
        assert_eq!((counter, frame), (3, 1));
        assert!((phase - 6.0).abs() < 1e-5);
        assert_eq!(step_water(0, 180.0, 3, 2.0).1, -178.0);
        let (counter, _, frame) = (0..96).fold((0, 0.0, 0), |(counter, phase, _), _| step_water(counter, phase, 3, 2.0));
        assert_eq!((counter, frame), (0, 0));
    }
}
