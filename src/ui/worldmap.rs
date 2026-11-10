//! The world map (the basic info menu's map button): the picture of the world the player is in, filling the screen's
//! height. The field the player is on has an outline and a face on it, the one pointed at is boxed and named, and a
//! navigation's route shows on it: a box on each of its maps, a strip from one to the next, and a mark on the last.

use std::collections::HashMap;

use image::RgbaImage;
use rustnarok_client::grf::Grf;
use rustnarok_client::lua::World;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture, load_image};
use crate::window::{self, Chrome};
use crate::{Layer, UI, contains, offset, screen};

const BUTTON: [f32; 2] = [11.0, 11.0];
/// The outline round the field the player is on.
const HERE: [u8; 4] = [232, 174, 35, 255];
/// A route strip: this far to each side of the line between two fields' middles, at this opacity; the mark on the
/// route's last field, the picture over the picked place's field and the player's face are see-through too.
const STRIP_HALF: f32 = 5.0;
const STRIP_ALPHA: u8 = 0x99;
const END_ALPHA: u8 = 0xCC;
const MARK_ALPHA: u8 = 0xEE;

/// A map of a navigation's route that a world shows: which world, its box there, and the strip to it from the map
/// before (the picture and its corner on screen) when that one is on the same world.
struct Stop {
    map: String,
    world: usize,
    rect: [f32; 4],
    strip: Option<(Texture, [f32; 2])>,
}

// ponytail: one world (the player's), no dungeon markers or world list; add them with worldtable_*_Dun and
// World_List when someone misses them.
pub struct WorldMap {
    world: usize,
    picture: Texture,
    /// Boxes: a route's maps, what's pointed at (worldview_info's Run and SelectBox colors); the outline's color.
    boxes: [Texture; 2],
    here: Texture,
    backdrop: Texture,
    /// The strip's picture, the route's end mark, the picked place's picture and the player's face (none when the
    /// data lacks it).
    band: Option<RgbaImage>,
    marks: [Option<Texture>; 3],
    /// The route as it was last given (`set_route`) with the screen's size then, and its maps on the worlds.
    route_for: (Vec<String>, [f32; 2]),
    route: Vec<Stop>,
}

/// The maps of a route that the world maps show, in order: each once, with the first world that has it as a town
/// or a field (a dungeon or a map inside is never one) and its box there.
pub fn stops<'a>(worlds: &'a [World], maps: &[&str]) -> Vec<(usize, &'a str, [f32; 4])> {
    let mut found: Vec<(usize, &str, [f32; 4])> = Vec::new();
    for map in maps {
        let on = worlds
            .iter()
            .enumerate()
            .find_map(|(index, world)| world.fields.iter().find(|field| field.map == *map && field.region >= 100).map(|field| (index, field)));
        if let Some((world, field)) = on.filter(|_| !found.iter().any(|(_, seen, _)| seen == map)) {
            found.push((world, &field.map, field.rect));
        }
    }
    found
}

/// A strip between two points as a picture, and its corner: `band` stretched once over it (its columns across the
/// strip's width, its rows along its length), square at both ends, see-through.
fn strip(band: &RgbaImage, from: [f32; 2], to: [f32; 2]) -> Option<(RgbaImage, [f32; 2])> {
    let along = [to[0] - from[0], to[1] - from[1]];
    let length = along[0].hypot(along[1]);
    if length == 0.0 {
        return None;
    }
    let corner = [from[0].min(to[0]) - STRIP_HALF, from[1].min(to[1]) - STRIP_HALF].map(f32::floor);
    let size = [along[0].abs() + 2.0 * STRIP_HALF, along[1].abs() + 2.0 * STRIP_HALF].map(|side| side.ceil() as u32 + 1);
    let picture = RgbaImage::from_fn(size[0], size[1], |x, y| {
        let point = [corner[0] + x as f32 + 0.5 - from[0], corner[1] + y as f32 + 0.5 - from[1]];
        let down = (point[0] * along[0] + point[1] * along[1]) / (length * length);
        let aside = (along[0] * point[1] - along[1] * point[0]) / length;
        if !(0.0..=1.0).contains(&down) || aside.abs() > STRIP_HALF {
            return image::Rgba([0; 4]);
        }
        let texel = |part: f32, count: u32| ((part * count as f32) as u32).min(count - 1);
        let image::Rgba([red, green, blue, alpha]) =
            *band.get_pixel(texel((aside + STRIP_HALF) / (2.0 * STRIP_HALF), band.width()), texel(down, band.height()));
        image::Rgba([red, green, blue, (u16::from(alpha) * u16::from(STRIP_ALPHA) / 255) as u8])
    });
    Some((picture, corner))
}

/// The side of the player's face on their field: bigger on a wider screen.
fn face_side(screen_width: f32) -> f32 {
    16.0 + 4.0 * [640.0, 800.0, 1024.0].iter().filter(|&&width| screen_width > width).count() as f32
}

impl WorldMap {
    /// The world holding `map`'s field (the first world when none does).
    pub fn open(gpu: &Gpu, grf: &Grf, worlds: &[World], colors: [[u8; 4]; 2], map: &str) -> Option<Self> {
        let world = worlds.iter().position(|world| world.fields.iter().any(|field| field.map == map)).unwrap_or(0);
        let picture = gpu.try_load_texture(grf, &format!("{UI}{}", worlds.get(world)?.image), true).ok()?;
        let image = |name: &str| load_image(grf, &format!("{UI}worldview_interface\\{name}.bmp")).ok();
        let faded = |name: &str, alpha: u8| {
            let mut image = image(name)?;
            image.pixels_mut().for_each(|pixel| pixel[3] = (u16::from(pixel[3]) * u16::from(alpha) / 255) as u8);
            Some(gpu.texture(&image, true))
        };
        Some(Self {
            world,
            picture,
            boxes: colors.map(|color| gpu.solid(color)),
            here: gpu.solid(HERE),
            backdrop: gpu.solid([0, 0, 0, 0xFF]),
            band: image("wv_Airf2"),
            marks: [faded("wv_End", END_ALPHA), faded("wv_Select", MARK_ALPHA), faded("wv_face1", MARK_ALPHA)],
            route_for: (Vec::new(), [0.0; 2]),
            route: Vec::new(),
        })
    }

    /// Takes a navigation's route (`maps`: the maps it goes through, the target's last; none when none runs) and
    /// makes its strips, when the route or the screen changed.
    pub fn set_route(&mut self, gpu: &Gpu, worlds: &[World], maps: &[&str]) {
        if self.route_for.0.iter().map(String::as_str).ne(maps.iter().copied()) || self.route_for.1 != screen() {
            self.route_for = (maps.iter().map(|&map| map.to_owned()).collect(), screen());
            let stops = stops(worlds, maps);
            let middle = |rect: [f32; 4]| {
                let (at, size) = self.place(rect);
                [at[0] + size[0] / 2.0, at[1] + size[1] / 2.0]
            };
            // A strip comes from the map before on the route's list, and only when both are on one world.
            let strips: Vec<_> = (0..stops.len())
                .map(|index| {
                    let before = stops[index.checked_sub(1)?];
                    let (picture, corner) = strip(self.band.as_ref()?, middle(before.2), middle(stops[index].2)).filter(|_| before.0 == stops[index].0)?;
                    Some((gpu.texture(&picture, true), corner))
                })
                .collect();
            self.route = stops.into_iter().zip(strips).map(|((world, map, rect), strip)| Stop { map: map.to_owned(), world, rect, strip }).collect();
        }
    }

    /// The route's maps on the shown world, for the test driver.
    pub fn route_shown(&self) -> Vec<&str> {
        self.route.iter().filter(|stop| stop.world == self.world).map(|stop| stop.map.as_str()).collect()
    }

    /// The picture's corner on screen, and its scale.
    fn placement(&self) -> ([f32; 2], f32) {
        let scale = (screen()[0] / self.picture.size[0]).min(screen()[1] / self.picture.size[1]);
        let size = self.picture.size.map(|side| side * scale);
        ([((screen()[0] - size[0]) / 2.0).floor(), ((screen()[1] - size[1]) / 2.0).floor()], scale)
    }

    fn close_button(&self) -> [f32; 2] {
        let (corner, scale) = self.placement();
        [corner[0] + self.picture.size[0] * scale - 16.0, corner[1] + 5.0]
    }

    pub fn closes(&self, cursor: [f32; 2]) -> bool {
        contains(self.close_button(), BUTTON, cursor)
    }

    /// A field's box on screen: corner and size.
    fn place(&self, rect: [f32; 4]) -> ([f32; 2], [f32; 2]) {
        let (corner, scale) = self.placement();
        ([corner[0] + rect[0] * scale, corner[1] + rect[1] * scale], [(rect[2] - rect[0]) * scale, (rect[3] - rect[1]) * scale])
    }

    /// `target` is the running navigation's target map and `picked` the map of the place picked for one.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &'a self,
        layer: &mut Layer<'a>,
        chrome: &'a Chrome,
        font: &Font,
        worlds: &[World],
        map_names: &HashMap<String, String>,
        map: &str,
        pointer: [f32; 2],
        target: Option<&str>,
        picked: Option<&str>,
    ) {
        let Some(world) = worlds.get(self.world) else { return };
        let (corner, scale) = self.placement();
        layer.sprites.push((&self.backdrop, [0.0, 0.0], screen()));
        layer.sprites.push((&self.picture, corner, self.picture.size.map(|side| side * scale)));
        // The route, a map at a time: its box a pixel inside the field's, the strip to it over both boxes, and on
        // the last one, when it's the target's, the end mark with its bottom right corner on the field's middle.
        for (index, stop) in self.route.iter().enumerate().filter(|(_, stop)| stop.world == self.world) {
            let (at, size) = self.place(stop.rect);
            layer.sprites.push((&self.boxes[0], offset(at, [1.0, 1.0]), [size[0] - 2.0, size[1] - 2.0]));
            if let Some((strip, corner)) = &stop.strip {
                layer.sprites.push((strip, *corner, strip.size));
            }
            if let Some(end) = self.marks[0].as_ref().filter(|_| index + 1 == self.route.len() && target == Some(stop.map.as_str())) {
                layer.sprites.push((end, [(at[0] + size[0] / 2.0 - end.size[0]).trunc(), (at[1] + size[1] / 2.0 - end.size[1]).trunc()], end.size));
            }
        }
        // The picked place's field under its picture; the player's face in the middle of theirs.
        for field in world.fields.iter().filter(|field| Some(field.map.as_str()) == picked) {
            let (at, size) = self.place(field.rect);
            layer.sprites.extend(self.marks[1].as_ref().map(|select| (select, at, size)));
        }
        for field in world.fields.iter().filter(|field| field.map == map) {
            let (at, size) = self.place(field.rect);
            let side = face_side(screen()[0]);
            layer.sprites.extend(self.marks[2].as_ref().map(|face| (face, [at[0] + (size[0] - side) / 2.0, at[1] + (size[1] - side) / 2.0], [side, side])));
        }
        let pointed = world.fields.iter().find(|field| {
            let (at, size) = self.place(field.rect);
            contains(at, size, pointer)
        });
        if let Some(field) = pointed {
            let (at, size) = self.place(field.rect);
            layer.sprites.push((&self.boxes[1], at, size));
        }
        // The outline round the player's field, its corners cut, over the rest.
        for field in world.fields.iter().filter(|field| field.map == map) {
            let ([x, y], [wide, high]) = self.place(field.rect);
            for line in [
                [x + 1.0, y, wide - 2.0, 1.0],
                [x + 1.0, y + high - 1.0, wide - 2.0, 1.0],
                [x, y + 1.0, 1.0, high - 2.0],
                [x + wide - 1.0, y + 1.0, 1.0, high - 2.0],
            ] {
                layer.sprites.push((&self.here, [line[0], line[1]], [line[2], line[3]]));
            }
        }
        window::tooltip(layer, chrome, font, &world.name, offset(corner, [8.0, 8.0]), [255; 3]);
        let close = self.close_button();
        let image = chrome.button(2, contains(close, BUTTON, pointer));
        layer.sprites.push((image, close, image.size));
        if let Some(field) = pointed {
            let name = map_names.get(&field.map).map_or(field.map.as_str(), String::as_str);
            window::tooltip(layer, chrome, font, name, offset(pointer, [14.0, 14.0]), [255; 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustnarok_client::lua::WorldArea;

    #[test]
    fn lists_a_routes_fields_and_draws_a_strip() {
        let field = |map: &str, region: u32| WorldArea { map: map.to_owned(), rect: [0.0, 0.0, 10.0, 10.0], region };
        let world = |fields: Vec<WorldArea>| World { name: String::new(), image: String::new(), fields };
        let worlds =
            [world(vec![field("prt_fild08", 108), field("prontera", 108), field("prt_maze01", 30)]), world(vec![field("prontera", 110), field("rachel", 120)])];
        // Towns and fields only, each once, on the first world that has it; a dungeon or an unknown map leaves no gap.
        let maps = ["prt_fild08", "prt_maze01", "prontera", "prt_in", "prontera", "rachel"];
        let found: Vec<_> = stops(&worlds, &maps).into_iter().map(|(world, map, _)| (world, map)).collect();
        assert_eq!(found, [(0, "prt_fild08"), (0, "prontera"), (1, "rachel")]);
        // A strip: ten wide round the line, square at its ends, the band's rows along it, see-through.
        let band = RgbaImage::from_fn(4, 4, |_, y| image::Rgba([y as u8 * 10, 0, 255, 255]));
        let (picture, corner) = strip(&band, [10.0, 20.0], [50.0, 20.0]).unwrap();
        assert_eq!((corner, picture.dimensions()), ([5.0, 15.0], (51, 11)));
        assert_eq!((picture.get_pixel(6, 5).0, picture.get_pixel(43, 5).0), ([0, 0, 255, 0x99], [30, 0, 255, 0x99]));
        assert_eq!((picture.get_pixel(2, 5).0[3], picture.get_pixel(48, 5).0[3], picture.get_pixel(25, 0).0[3]), (0, 0, 0x99));
        assert!(strip(&band, [1.0, 1.0], [1.0, 1.0]).is_none());
        // The face grows with the screen's width.
        assert_eq!([640.0, 800.0, 1024.0, 1280.0].map(face_side), [16.0, 20.0, 24.0, 28.0]);
    }
}
