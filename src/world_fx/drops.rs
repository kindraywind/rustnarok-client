//! Item drop pillars: the coloured light column a dropped item stands under until it is picked up, effect ids 1186 (pink),
//! 1189 (yellow), 1190 (purple), 1869 (blue), 1870 (green) and 1871 (red).

use super::{Assets, Class, FLAGS_FLOOR, Face, Fx, Launch, Seen, StrPart, WorldFx};
use crate::world::Vector;
use crate::world_str::Scene;

/// The pillar ids and their colours' names.
const PILLARS: [(u32, &str); 6] = [(1186, "pink"), (1189, "yellow"), (1190, "purple"), (1869, "blue"), (1870, "green"), (1871, "red")];

/// The colour name of a pillar id.
fn colour_of(effect: u32) -> Option<&'static str> {
    PILLARS.iter().find(|(id, _)| *id == effect).map(|(_, colour)| *colour)
}

/// Whether an effect id is a pillar: a server effect packet for it plays the effect rows, not a pillar.
pub fn is_pillar(effect: u32) -> bool {
    colour_of(effect).is_some()
}

/// The pillar a fall packet's drop effect mode (1 … 5) names.
fn pillar_of_mode(mode: u16) -> Option<u32> {
    match mode {
        1 => Some(1869),
        2 => Some(1189),
        3 => Some(1190),
        4 => Some(1870),
        5 => Some(1871),
        _ => None,
    }
}

/// What a dropped item launches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    /// A pillar that stands while the item lies.
    Pillar(u32),
    /// Any other effect number of the item info, played once at the item (2371 and 2372 are STR rows).
    Effect(u32),
}

/// The launches of a fall packet for an item: the item info's effect number, then the mode's pillar. A packet with show
/// drop effect 0 launches nothing.
pub fn launches(info: Option<u32>, show: bool, mode: u16) -> Vec<Drop> {
    if !show {
        return Vec::new();
    }
    // ponytail: an effect number of 0 is taken as none.
    let own = info.filter(|&effect| effect != 0).map(|effect| if is_pillar(effect) { Drop::Pillar(effect) } else { Drop::Effect(effect) });
    own.into_iter().chain(pillar_of_mode(mode).map(Drop::Pillar)).collect()
}

/// Starts pillar `effect` for the ground item `item`, standing at `at`.
pub fn stand(fx: &mut WorldFx, effect: u32, item: u32, at: Vector, assets: &mut dyn Assets) {
    fx.launch(&Launch::at(effect, Some(Some(item)), at), assets);
}

/// Registers the pillars.
pub fn register(fx: &mut WorldFx) {
    for (id, _) in PILLARS {
        fx.register(id, || Box::new(Pillar::default()));
    }
}

/// The two STRs of a pillar: the light column with its sparkles, and the glow on the ground, both starting again at their
/// end, drawn on the floor list at `at`.
fn parts(effect: u32, at: Vector, assets: &mut dyn Assets) -> Vec<StrPart> {
    let Some(colour) = colour_of(effect) else { return Vec::new() };
    let base = format!("new_dropitem\\dropitem_{colour}");
    [format!("{base}\\dropitem_{colour}\\dropitem_{colour}"), format!("{base}\\dropitem_{colour}_bottom\\dropitem_{colour}_bottom")]
        .iter()
        .map(|file| {
            let mut part = StrPart::new(file, false, assets);
            part.flags = FLAGS_FLOOR;
            part.offset = [0.0; 2];
            part.timeline.looped = true;
            part.point = at;
            part
        })
        .collect()
}

/// The pillar of one item, which it owns: it stands at the item and goes with it.
#[derive(Default)]
struct Pillar {
    parts: Vec<StrPart>,
}

impl Class for Pillar {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        self.parts = parts(fx.id, fx.position, assets);
    }

    fn update(&mut self, fx: &mut Fx, _: &Scene, _: &mut dyn Assets) {
        for part in &mut self.parts {
            part.point = fx.position;
            part.step();
        }
    }

    fn draw<'a>(&'a self, _: &Fx, seen: &Seen, out: &mut Vec<Face<'a>>) {
        for part in &self.parts {
            part.faces(&seen.eye, out);
        }
    }

    /// Stands while its item lies on the ground.
    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Eye, List, Lists, Tex};
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use crate::world_str::Fired;
    use rustnarok_client::effect::{Key, Layer, Str};
    use std::rc::Rc;

    const ITEM: u32 = 0x460;
    const AT: Vector = [30.0, 4.0, 50.0];

    /// Every STR is 60 frames (keys at 0 and 59) of one layer; the files asked for are kept.
    #[derive(Default)]
    struct Fake {
        asked: Vec<String>,
        missing: bool,
    }

    fn key(frame: i32) -> Key {
        Key {
            frame,
            kind: 0,
            position: [340.0, 230.0],
            corners: [-50.0, 50.0, 50.0, -50.0, -25.0, -25.0, 25.0, 25.0],
            texture: 0.0,
            texture_motion: 0,
            delay: 0.0,
            angle: 0.0,
            color: [1.0; 4],
            source_blend: 5,
            destination_blend: 2,
        }
    }

    impl Assets for Fake {
        fn texture(&mut self, _: &str) -> Option<[f32; 2]> {
            None
        }
        fn str(&mut self, file: &str) -> Option<Rc<Loaded>> {
            self.asked.push(file.to_owned());
            if self.missing {
                return None;
            }
            let layer = || Layer { textures: vec!["a.bmp".into()], keys: vec![key(0), key(59)] };
            Some(Rc::new(Loaded { animation: Str { fps: 60, frames: 59, layers: vec![layer(), layer()] }, textures: vec![Vec::new(); 2] }))
        }
        fn sprite(&mut self, _: &str, _: usize, _: usize) -> Option<(usize, Vec<Placed>)> {
            None
        }
    }

    /// One tick with `lying` the ground items' ids (each at `AT`).
    fn tick(fx: &mut WorldFx, lying: &[u32], assets: &mut Fake) -> Vec<Fired> {
        let feet = |unit: Option<u32>| match unit {
            Some(id) if lying.contains(&id) => Some(AT),
            Some(_) => None,
            None => Some([0.0; 3]),
        };
        let body = |_: Option<u32>| None;
        let eye = Eye::at(45.0, 0.0);
        let scene = Scene { player: [0.0; 3], feet: &feet, body: &body, longitude: 0.0, right: eye.right, up: eye.up, ground: &|_, _| 0.0 };
        fx.tick(&scene, assets)
    }

    fn drawn(fx: &WorldFx, effects_off: bool) -> Lists<Face<'_>> {
        // The data's force-drawn rule: the pillar ids are always shown.
        let shows = move |id: u32| !effects_off || crate::effects::force_drawn(id);
        let hidden = |_: Option<u32>| false;
        let seen = Seen { eye: Eye::at(45.0, 0.0), shows: &shows, hidden: &hidden, size: &|_| 1.0, ground: &|_, _| 0.0, dim: false };
        Lists::sort(fx.faces(&seen), |_| 0.0)
    }

    fn stood(fx: &mut WorldFx, effect: u32, item: u32, assets: &mut Fake) {
        stand(fx, effect, item, AT, assets);
    }

    #[test]
    fn the_launch_table() {
        let pillar = |ids: &[u32]| ids.iter().map(|&id| Drop::Pillar(id)).collect::<Vec<_>>();
        // The item info's number alone, then the mode's pillar alone.
        assert_eq!(launches(Some(1186), true, 0), pillar(&[1186]));
        for (mode, id) in [(1, 1869), (2, 1189), (3, 1190), (4, 1870), (5, 1871)] {
            assert_eq!(launches(None, true, mode), pillar(&[id]), "mode {mode}");
        }
        // Both apply: two pillars.
        assert_eq!(launches(Some(1186), true, 5), pillar(&[1186, 1871]));
        // Show drop effect 0 launches nothing, whatever the item info says.
        assert!(launches(Some(1186), false, 5).is_empty());
        assert!(launches(None, false, 3).is_empty());
        // The STR rows 2371 and 2372 are played at the item, once; mode 0 and a mode that is no colour add none.
        assert_eq!(launches(Some(2371), true, 0), [Drop::Effect(2371)]);
        assert_eq!(launches(Some(2372), true, 6), [Drop::Effect(2372)]);
        assert!(launches(None, true, 0).is_empty());
        assert!(launches(None, true, 6).is_empty());
        // Any other number goes the way an unknown effect id goes; 0 is none.
        assert_eq!(launches(Some(500), true, 1), [Drop::Effect(500), Drop::Pillar(1869)]);
        assert!(launches(Some(0), true, 0).is_empty());
    }

    #[test]
    fn a_pillar_is_two_looping_floor_parts_at_the_item() {
        let mut assets = Fake::default();
        for (id, colour) in PILLARS {
            let parts = parts(id, AT, &mut assets);
            assert_eq!(parts.len(), 2, "{colour}");
            for part in &parts {
                assert_eq!((part.flags, part.offset, part.point, part.timeline.looped, part.world), (FLAGS_FLOOR, [0.0, 0.0], AT, true, false), "{colour}");
            }
        }
        assert_eq!(
            assets.asked[..2],
            ["new_dropitem\\dropitem_pink\\dropitem_pink\\dropitem_pink", "new_dropitem\\dropitem_pink\\dropitem_pink_bottom\\dropitem_pink_bottom"]
        );
        assert_eq!(
            assets.asked[10..12],
            ["new_dropitem\\dropitem_red\\dropitem_red\\dropitem_red", "new_dropitem\\dropitem_red\\dropitem_red_bottom\\dropitem_red_bottom"]
        );
        assert!(parts(2371, AT, &mut assets).is_empty());
    }

    #[test]
    fn the_parts_start_again_at_frame_zero_on_tick_60() {
        let mut part = parts(1186, AT, &mut Fake::default()).remove(0);
        let mut shown = Vec::new();
        for _ in 0..=61 {
            assert!(part.step());
            shown.push(part.shown.unwrap());
        }
        assert_eq!((shown[0], shown[59], shown[60], shown[61]), (0, 59, 0, 1));
    }

    #[test]
    fn it_stands_while_its_item_lies() {
        let mut assets = Fake::default();
        let mut fx = WorldFx::default();
        stood(&mut fx, 1186, ITEM, &mut assets);
        // Both STRs are drawn, on the floor list, from the first tick on, and are alive far past one play.
        assert_eq!(drawn(&fx, false).floor.len(), 0);
        for _ in 0..1_000 {
            tick(&mut fx, &[ITEM], &mut assets);
        }
        assert_eq!(fx.iter().count(), 1);
        let lists = drawn(&fx, false);
        assert_eq!((lists.floor.len(), lists.opaque.len() + lists.effect.len() + lists.translucent.len()), (2, 0));
        assert_eq!(List::of(super::FLAGS_FLOOR), List::Floor);
        // It stays at the item.
        assert_eq!(fx.iter().next().unwrap().fx.position, AT);
        // The item gone from the ground takes it at the next tick.
        tick(&mut fx, &[], &mut assets);
        assert!(fx.is_empty());
    }

    #[test]
    fn item_gone_removes_that_items_pillars_only() {
        let mut assets = Fake::default();
        let mut fx = WorldFx::default();
        stood(&mut fx, 1186, ITEM, &mut assets);
        stood(&mut fx, 1871, ITEM, &mut assets);
        stood(&mut fx, 1186, ITEM + 1, &mut assets);
        // Not unique: a second fall packet makes its own copy.
        assert_eq!(fx.iter().count(), 3);
        fx.remove_all_of(Some(Some(ITEM)));
        assert_eq!(fx.iter().map(|object| object.fx.owner).collect::<Vec<_>>(), [Some(Some(ITEM + 1))]);
        tick(&mut fx, &[ITEM + 1], &mut assets);
        assert_eq!(fx.iter().count(), 1);
        // A map change removes all.
        fx.remove_all();
        assert!(fx.is_empty());
    }

    #[test]
    fn it_draws_with_effect_off_and_without_a_sound() {
        let mut assets = Fake::default();
        let mut fx = WorldFx::default();
        stood(&mut fx, 1189, ITEM, &mut assets);
        assert!(tick(&mut fx, &[ITEM], &mut assets).is_empty());
        assert_eq!(drawn(&fx, true).floor.len(), 2);
        // The pillar's faces use the STR layers' textures.
        let lists = drawn(&fx, true);
        assert!(lists.floor.iter().all(|face| matches!(face.texture, Tex::Str(..))));
    }

    #[test]
    fn a_pillar_whose_files_are_missing_stands_and_draws_nothing() {
        let mut assets = Fake { missing: true, ..Fake::default() };
        let mut fx = WorldFx::default();
        stood(&mut fx, 1870, ITEM, &mut assets);
        tick(&mut fx, &[ITEM], &mut assets);
        assert_eq!(fx.iter().count(), 1);
        assert!(drawn(&fx, false).floor.is_empty());
    }

    #[test]
    fn the_effect_rows_keep_their_str_files_and_have_no_sound_rows() {
        let table = include_str!("../effects.txt");
        let rows = |kind: &str, id: u32| table.lines().filter(|line| line.split('\t').take(2).eq([kind, &id.to_string()])).count();
        for (id, _) in PILLARS {
            assert_eq!(rows("str", id), 2, "{id}");
            assert_eq!(rows("wav", id), 0, "{id}");
            assert!(WorldFx::default().registered(id));
        }
    }
}
