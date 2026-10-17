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
}

pub struct Rsw {
    pub ground_file: String,
    pub altitude_file: String,
    pub water: Option<Water>,
    pub light: Light,
    pub models: Vec<Model>,
}

impl Rsw {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let mut reader = Reader { bytes, position: 0 };
        let version = reader.header(b"GRSW")?;
        let build = if version >= (2, 5) { reader.i32()? } else { 0 };
        if version >= (2, 2) {
            reader.u8()?;
        }
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

        let mut models = Vec::new();
        for _ in 0..reader.i32()?.max(0) {
            match reader.i32()? {
                1 => {
                    let name = if version >= (1, 3) {
                        let name = reader.string(40)?;
                        reader.take(12)?; // animation type, animation speed, block type
                        name
                    } else {
                        String::new()
                    };
                    if version >= (2, 6) && build >= 186 {
                        reader.u8()?;
                    }
                    let file = reader.string(80)?;
                    reader.string(80)?; // node name
                    models.push(Model { name, file, position: reader.vector()?, rotation: reader.vector()?, scale: reader.vector()? });
                }
                // Light sources, sounds and effects aren't drawn yet; their sizes are fixed.
                2 => drop(reader.take(80 + 12 + 12 + 4)?),
                3 => drop(reader.take(80 + 80 + 12 + 4 + 4 + 4 + 4 + if version >= (2, 0) { 4 } else { 0 })?),
                4 => drop(reader.take(80 + 12 + 4 + 4 + 16)?),
                other => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown RSW object type {other}"))),
            }
        }
        Ok(Self { ground_file, altitude_file, water, light, models })
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
        Ok(Self { width, height, zoom, textures, lightmap_size, lightmaps, surfaces, cubes })
    }
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

    /// Types 0 (walkable), 3 (walkable water) can be walked on.
    pub fn walkable(&self, [x, y]: [i32; 2]) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height && matches!(self.cells[y as usize * self.width + x as usize].1, 0 | 3)
    }

    /// Average height of a cell (negative is up).
    pub fn height(&self, [x, y]: [i32; 2]) -> f32 {
        let x = x.clamp(0, self.width as i32 - 1) as usize;
        let y = y.clamp(0, self.height as i32 - 1) as usize;
        self.cells[y * self.width + x].0.iter().sum::<f32>() / 4.0
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the official data.grf, at `ROCLIENT_GRF` or `./data.grf`.
    #[test]
    fn parses_prontera() {
        let grf = Grf::open(std::env::var("ROCLIENT_GRF").unwrap_or_else(|_| "data.grf".into())).unwrap();
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
}
