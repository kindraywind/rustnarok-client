//! The world map (the basic info menu's map button): the picture of the world the player is in, filling the screen's
//! height, with the field they're on boxed in the client's color for it and the one pointed at boxed and named.

use std::collections::HashMap;

use rustnarok_client::grf::Grf;
use rustnarok_client::lua::World;
use rustnarok_client::text::Font;

use crate::render::{Gpu, Texture};
use crate::window::{self, Chrome};
use crate::{Layer, SCREEN, UI, contains, offset};

const BUTTON: [f32; 2] = [11.0, 11.0];

// ponytail: one world (the player's), no dungeon markers or world list; add them with worldtable_*_Dun and
// World_List when someone misses them.
pub struct WorldMap {
    world: usize,
    picture: Texture,
    /// Boxes: where the player is, what's pointed at (worldview_info's Run and SelectBox colors).
    boxes: [Texture; 2],
    backdrop: Texture,
}

impl WorldMap {
    /// The world holding `map`'s field (the first world when none does).
    pub fn open(gpu: &Gpu, grf: &Grf, worlds: &[World], colors: [[u8; 4]; 2], map: &str) -> Option<Self> {
        let world = worlds.iter().position(|world| world.fields.iter().any(|field| field.map == map)).unwrap_or(0);
        let picture = gpu.try_load_texture(grf, &format!("{UI}{}", worlds.get(world)?.image), true).ok()?;
        Some(Self { world, picture, boxes: colors.map(|color| gpu.solid(color)), backdrop: gpu.solid([0, 0, 0, 0xFF]) })
    }

    /// The picture's corner on screen, and its scale.
    fn placement(&self) -> ([f32; 2], f32) {
        let scale = (SCREEN[0] / self.picture.size[0]).min(SCREEN[1] / self.picture.size[1]);
        let size = self.picture.size.map(|side| side * scale);
        ([((SCREEN[0] - size[0]) / 2.0).floor(), ((SCREEN[1] - size[1]) / 2.0).floor()], scale)
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
    ) {
        let Some(world) = worlds.get(self.world) else { return };
        let (corner, scale) = self.placement();
        layer.sprites.push((&self.backdrop, [0.0, 0.0], SCREEN));
        layer.sprites.push((&self.picture, corner, self.picture.size.map(|side| side * scale)));
        for field in world.fields.iter().filter(|field| field.map == map) {
            let (at, size) = self.place(field.rect);
            layer.sprites.push((&self.boxes[0], at, size));
        }
        let pointed = world.fields.iter().find(|field| {
            let (at, size) = self.place(field.rect);
            contains(at, size, pointer)
        });
        window::tooltip(layer, chrome, font, &world.name, offset(corner, [8.0, 8.0]), [255; 3]);
        let close = self.close_button();
        let image = chrome.button(2, contains(close, BUTTON, pointer));
        layer.sprites.push((image, close, image.size));
        if let Some(field) = pointed {
            let (at, size) = self.place(field.rect);
            layer.sprites.push((&self.boxes[1], at, size));
            let name = map_names.get(&field.map).map_or(field.map.as_str(), String::as_str);
            window::tooltip(layer, chrome, font, name, offset(pointer, [14.0, 14.0]), [255; 3]);
        }
    }
}
