//! Footprint hats: the hat effect packet's numbers that leave prints on the ground instead of playing an STR on the
//! unit, and class 2411, the picture footprints of hat effect 226.

use super::{Assets, Class, FLAGS_TRANSLUCENT, Face, Fx, Launch, Quad, Seen, WorldFx, aim_turn, exact_uv};
use crate::world::Vector;
use crate::world_str::Scene;

/// The class of hat effect 226: left and right pictures.
pub const PICTURES: u32 = 2411;
/// The class of the STR footprint hats (a later issue draws it).
pub const STRS: u32 = 2412;

/// The hat effect numbers whose class is `STRS`.
const STR_HATS: [u16; 24] = [227, 228, 229, 230, 240, 244, 245, 246, 257, 258, 259, 260, 261, 263, 264, 265, 271, 272, 273, 280, 283, 285, 286, 287];

/// The footprint class of a hat effect number, None for any other hat.
pub fn class_of(hat: u16) -> Option<u32> {
    match hat {
        226 => Some(PICTURES),
        _ if STR_HATS.contains(&hat) => Some(STRS),
        _ => None,
    }
}

/// Whether an effect id is a footprint class: a server effect packet for it has no hat number and draws nothing.
pub fn is_class(effect: u32) -> bool {
    matches!(effect, PICTURES | STRS)
}

/// A hat effect packet for a unit: each footprint hat starts (on) or removes (off) its class's copy, tagged with the
/// hat's number; other hats are not this module's.
pub fn hats(fx: &mut WorldFx, owner: Option<Option<u32>>, on: bool, hats: &[u16], assets: &mut dyn Assets) {
    for &hat in hats {
        let Some(class) = class_of(hat) else { continue };
        let tag = u32::from(hat);
        if on {
            fx.launch(&Launch { tag, ..Launch::at(class, owner, [0.0; 3]) }, assets);
        } else {
            fx.remove_tagged(owner, class, tag);
        }
    }
}

/// Registers the footprint classes.
pub fn register(fx: &mut WorldFx) {
    fx.register(PICTURES, || Box::new(Pictures::default()));
}

const LEFT: &str = "data\\texture\\effect\\footprint0.png";
const RIGHT: &str = "data\\texture\\effect\\footprint1.png";
/// A print's half-side, its alpha at the start, its life in seconds, and the squared ground distance a step must pass.
const SCALE: f32 = 5.0;
const ALPHA: f32 = 250.0;
const DURATION: f32 = 10.0;
const STRIDE: f32 = 50.0;
/// A print lies this far over the ground under each corner.
const LIFT: f32 = 2.0;

/// The alpha of a print `age` ticks old (it fades by alpha₀ / (60 · duration) a tick, never below 0).
fn alpha_at(age: u32) -> f32 {
    (ALPHA - age as f32 * ALPHA / (60.0 * DURATION)).max(0.0)
}

/// The age (ticks) a print is removed at.
fn gone(age: u32) -> bool {
    age as f32 >= 60.0 * DURATION + 25.0
}

struct Print {
    quad: Quad,
    age: u32,
}

/// The prints of one unit.
#[derive(Default)]
struct Pictures {
    /// Where the last print was made (or the unit began), None before the first update.
    last: Option<Vector>,
    /// The next print is a left one.
    left: bool,
    /// The pictures' texture rectangles, left then right.
    uv: [[f32; 4]; 2],
    /// A picture could not be loaded: no print is made.
    missing: bool,
    prints: Vec<Print>,
}

impl Pictures {
    /// A print at `at`, turned toward the step from `last`.
    fn print(&self, last: Vector, at: Vector, scene: &Scene) -> Print {
        let mut quad = Quad::new(if self.left { LEFT } else { RIGHT });
        quad.uv = self.uv[usize::from(!self.left)];
        quad.additive = false;
        quad.flags = FLAGS_TRANSLUCENT;
        quad.scale = SCALE;
        quad.alpha = ALPHA.round() as i32;
        let step = [at[0] - last[0], 0.0, at[2] - last[2]];
        // The step's screen direction (y down), once; the picture's top edge turns to it.
        let screen = |axis: Vector| axis.iter().zip(step).map(|(a, b)| a * b).sum::<f32>();
        // ponytail: the picture's own orientation and the sign of the turn are unchecked against the game: flip if a walk shows them backward.
        let heading = aim_turn([0.0; 2], [screen(scene.right), -screen(scene.up)]) + 90.0;
        let (sin, cos) = (heading + scene.longitude).to_radians().sin_cos();
        quad.place(at, [cos, 0.0, -sin], [sin, 0.0, cos]);
        for corner in &mut quad.corners {
            corner[1] = (scene.ground)(corner[0], corner[2]) + LIFT;
        }
        Print { quad, age: 0 }
    }
}

impl Class for Pictures {
    fn begin(&mut self, fx: &mut Fx, assets: &mut dyn Assets) {
        fx.single = true;
        self.left = true;
        // Both pictures are needed; one that is missing leaves the prints unmade.
        let sizes = [LEFT, RIGHT].map(|path| assets.texture(path));
        self.missing = sizes.iter().any(Option::is_none);
        self.uv = sizes.map(|size| exact_uv(size.unwrap_or([1.0; 2])));
    }

    fn update(&mut self, fx: &mut Fx, scene: &Scene, _: &mut dyn Assets) {
        for print in &mut self.prints {
            print.age += 1;
            print.quad.alpha = alpha_at(print.age).round() as i32;
        }
        self.prints.retain(|print| !gone(print.age));
        let at = fx.position;
        let last = *self.last.get_or_insert(at);
        let (dx, dz) = (at[0] - last[0], at[2] - last[2]);
        if !self.missing && dx * dx + dz * dz > STRIDE {
            let print = self.print(last, at, scene);
            self.prints.push(print);
            self.left = !self.left;
            self.last = Some(at);
        }
    }

    fn draw<'a>(&'a self, _: &Fx, _: &Seen, out: &mut Vec<Face<'a>>) {
        out.extend(self.prints.iter().filter(|print| alpha_at(print.age) > 0.0).map(|print| print.quad.face()));
    }

    /// Ends with its unit, taking its prints.
    fn alive(&self, fx: &Fx, scene: &Scene) -> bool {
        fx.owned(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Eye;
    use super::*;
    use crate::actor::Placed;
    use crate::effects::Loaded;
    use crate::world_str::Fired;
    use std::rc::Rc;

    struct Fake;

    impl Assets for Fake {
        fn texture(&mut self, _: &str) -> Option<[f32; 2]> {
            Some([32.0; 2])
        }
        fn str(&mut self, _: &str) -> Option<Rc<Loaded>> {
            None
        }
        fn sprite(&mut self, _: &str, _: usize, _: usize) -> Option<(usize, Vec<Placed>)> {
            None
        }
    }

    const UNIT: u32 = 7;

    fn registry() -> WorldFx {
        let mut fx = WorldFx::default();
        // The STR class has no object yet; a stand-in shows what the table launches.
        fx.register(STRS, || Box::new(Pictures::default()));
        fx
    }

    /// One tick with the unit at `at` and a ground that is `slope` high per unit of x.
    fn tick(fx: &mut WorldFx, at: Option<Vector>, eye: &Eye, longitude: f32, slope: f32) -> Vec<Fired> {
        let feet = |unit: Option<u32>| if unit == Some(UNIT) { at } else { Some([0.0; 3]) };
        let body = |_: Option<u32>| None;
        let ground = move |x: f32, _: f32| x * slope;
        let scene = Scene { player: [0.0; 3], feet: &feet, body: &body, longitude, right: eye.right, up: eye.up, ground: &ground };
        fx.tick(&scene, &mut Fake)
    }

    fn walk(fx: &mut WorldFx, at: Vector) {
        tick(fx, Some(at), &Eye::at(45.0, 0.0), 0.0, 0.0);
    }

    fn pictures(fx: &WorldFx) -> Vec<(String, f32)> {
        let shows = |_: u32| true;
        let hidden = |_: Option<u32>| false;
        let seen = Seen { eye: Eye::at(45.0, 0.0), shows: &shows, hidden: &hidden, size: &|_| 1.0 };
        fx.faces(&seen)
            .iter()
            .map(|face| {
                let super::super::Tex::Path(path) = face.texture else { panic!() };
                (path.to_owned(), face.vertices[0].tint[3] * 255.0)
            })
            .collect()
    }

    fn start(fx: &mut WorldFx, at: Vector) -> u32 {
        hats(fx, Some(Some(UNIT)), true, &[226], &mut Fake);
        walk(fx, at);
        assert_eq!(fx.iter().count(), 1);
        fx.iter().next().unwrap().serial
    }

    #[test]
    fn the_table_maps_hat_numbers_to_classes() {
        assert_eq!(class_of(226), Some(PICTURES));
        assert_eq!((class_of(227), class_of(287)), (Some(STRS), Some(STRS)));
        for hat in STR_HATS {
            assert_eq!(class_of(hat), Some(STRS), "{hat}");
        }
        assert_eq!(STR_HATS.len(), 24);
        assert!(STR_HATS.windows(2).all(|pair| pair[0] < pair[1]));
        for hat in [225, 231, 288, 0, 5] {
            assert_eq!(class_of(hat), None, "{hat}");
        }
        assert!(is_class(2411) && is_class(2412) && !is_class(2413));
    }

    #[test]
    fn on_launches_with_the_tag_and_off_removes_that_tag_only() {
        let mut fx = registry();
        let owner = Some(Some(UNIT));
        // Hats that are not footprint hats start nothing.
        hats(&mut fx, owner, true, &[5, 225], &mut Fake);
        assert!(fx.is_empty());
        hats(&mut fx, owner, true, &[226, 230, 5], &mut Fake);
        let tags: Vec<_> = fx.iter().map(|object| (object.fx.id, object.fx.tag)).collect();
        // 226 starts a picture copy; 230 an STR one; a second hat of one class starts nothing.
        assert_eq!(tags, [(PICTURES, 226), (STRS, 230)]);
        hats(&mut fx, owner, true, &[227], &mut Fake);
        assert_eq!(fx.iter().count(), 2);
        // Off removes the copy with that tag only.
        hats(&mut fx, owner, false, &[227], &mut Fake);
        assert_eq!(fx.iter().count(), 2);
        hats(&mut fx, owner, false, &[230, 5], &mut Fake);
        assert_eq!(fx.iter().map(|object| object.fx.tag).collect::<Vec<_>>(), [226]);
        hats(&mut fx, Some(Some(8)), false, &[226], &mut Fake);
        assert_eq!(fx.iter().count(), 1);
        hats(&mut fx, owner, false, &[226], &mut Fake);
        assert!(fx.is_empty());
    }

    #[test]
    fn it_follows_its_unit_and_dies_with_it() {
        let mut fx = registry();
        let serial = start(&mut fx, [20.0, 0.0, 20.0]);
        walk(&mut fx, [60.0, 0.0, 20.0]);
        assert_eq!(fx.get_mut(serial).unwrap().fx.position, [60.0, 0.0, 20.0]);
        assert_eq!(pictures(&fx).len(), 1);
        tick(&mut fx, None, &Eye::at(45.0, 0.0), 0.0, 0.0);
        assert!(fx.is_empty());
        assert!(pictures(&fx).is_empty());
    }

    #[test]
    fn a_print_needs_a_step_over_the_stride() {
        let mut fx = registry();
        start(&mut fx, [0.0, 0.0, 0.0]);
        // Squared length 50 makes none; 50.1 makes one. The first update only remembers where the unit is.
        walk(&mut fx, [50f32.sqrt(), 0.0, 0.0]);
        assert!(pictures(&fx).is_empty());
        walk(&mut fx, [50.1f32.sqrt(), 0.0, 0.0]);
        assert_eq!(pictures(&fx).len(), 1);
        // Last moved with the print: another half stride from there makes none.
        walk(&mut fx, [50.1f32.sqrt() + 3.0, 0.0, 0.0]);
        assert_eq!(pictures(&fx).len(), 1);
    }

    #[test]
    fn prints_alternate_left_and_right_and_last_moves_only_with_a_print() {
        let mut fx = registry();
        start(&mut fx, [0.0, 0.0, 0.0]);
        walk(&mut fx, [5.0, 0.0, 0.0]);
        // 5 units from the start is under the stride; the print comes at 8, measured from the start.
        assert!(pictures(&fx).is_empty());
        walk(&mut fx, [8.0, 0.0, 0.0]);
        walk(&mut fx, [10.0, 0.0, 0.0]);
        assert_eq!(pictures(&fx).len(), 1);
        // 8 → 16 is a step of 8 from the last print.
        walk(&mut fx, [16.0, 0.0, 0.0]);
        walk(&mut fx, [24.0, 0.0, 0.0]);
        let names: Vec<_> = pictures(&fx).into_iter().map(|(path, _)| path).collect();
        assert_eq!(names, [LEFT, RIGHT, LEFT]);
    }

    #[test]
    fn a_print_lies_on_a_slope() {
        let mut fx = registry();
        start(&mut fx, [20.0, 0.0, 20.0]);
        let eye = Eye::at(45.0, 30.0);
        tick(&mut fx, Some([30.0, 0.0, 20.0]), &eye, 30.0, 0.5);
        let shows = |_: u32| true;
        let hidden = |_: Option<u32>| false;
        let seen = Seen { eye, shows: &shows, hidden: &hidden, size: &|_| 1.0 };
        let faces = fx.faces(&seen);
        assert_eq!(faces.len(), 1);
        // Each corner is on the ground under it (x · slope) + 2, the square 5 from the unit's place at its middle.
        let corners: Vec<Vector> = [0usize, 1, 2, 5].iter().map(|&i| faces[0].vertices[i].position).collect();
        for corner in &corners {
            assert!((corner[1] - (corner[0] * 0.5 + 2.0)).abs() < 1e-3, "{corner:?}");
        }
        let centre: Vector = std::array::from_fn(|axis| corners.iter().map(|corner| corner[axis]).sum::<f32>() / 4.0);
        assert!((centre[0] - 30.0).abs() < 1e-3 && (centre[2] - 20.0).abs() < 1e-3);
    }

    #[test]
    fn the_top_edge_points_along_the_step_at_any_longitude() {
        for longitude in [0.0f32, 30.0, 90.0, 200.0] {
            let eye = Eye::at(45.0, longitude);
            for step in [eye.ground_right(), eye.ground_forward(), [-eye.ground_right()[0], 0.0, -eye.ground_right()[2]]] {
                let mut fx = registry();
                start(&mut fx, [0.0, 0.0, 0.0]);
                let at = [step[0] * 10.0, 0.0, step[2] * 10.0];
                tick(&mut fx, Some(at), &eye, longitude, 0.0);
                let shows = |_: u32| true;
                let hidden = |_: Option<u32>| false;
                let seen = Seen { eye, shows: &shows, hidden: &hidden, size: &|_| 1.0 };
                let faces = fx.faces(&seen);
                let [v0, v1] = [faces[0].vertices[0].position, faces[0].vertices[1].position];
                let top = [(v0[0] + v1[0]) / 2.0 - at[0], (v0[2] + v1[2]) / 2.0 - at[2]];
                assert!((top[0] - step[0] * 5.0).abs() < 1e-2 && (top[1] - step[2] * 5.0).abs() < 1e-2, "{longitude} {step:?} {top:?}");
            }
        }
    }

    #[test]
    fn a_print_fades_and_goes() {
        let alphas: Vec<f32> = [0, 1, 599, 600].map(alpha_at).to_vec();
        for (got, want) in alphas.iter().zip([250.0, 249.5833, 0.4167, 0.0]) {
            assert!((got - want).abs() < 1e-3, "{got} {want}");
        }
        assert!(!gone(624) && gone(625));

        let mut fx = registry();
        start(&mut fx, [0.0, 0.0, 0.0]);
        walk(&mut fx, [10.0, 0.0, 0.0]);
        assert_eq!(pictures(&fx)[0].1.round(), 250.0);
        // The unit stands where it made the print; the print does not move and fades.
        for _ in 0..599 {
            walk(&mut fx, [10.0, 0.0, 0.0]);
        }
        assert_eq!(pictures(&fx).len(), 1);
        walk(&mut fx, [10.0, 0.0, 0.0]);
        assert!(pictures(&fx).is_empty());
        let serial = fx.iter().next().unwrap().serial;
        assert_eq!(fx.get_mut(serial).unwrap().fx.position, [10.0, 0.0, 0.0]);
        // The print is removed at 625 ticks old.
        assert_eq!(fx.iter().count(), 1);
    }

    #[test]
    fn nothing_draws_while_the_body_is_hidden() {
        let mut fx = registry();
        start(&mut fx, [0.0, 0.0, 0.0]);
        walk(&mut fx, [10.0, 0.0, 0.0]);
        let shows = |_: u32| true;
        let hidden = |unit: Option<u32>| unit == Some(UNIT);
        let seen = Seen { eye: Eye::at(45.0, 0.0), shows: &shows, hidden: &hidden, size: &|_| 1.0 };
        assert!(fx.faces(&seen).is_empty());
    }
}
