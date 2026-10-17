//! Weather a map has on (rathena's snow, sakura, leaves, clouds, clouds2, fog and fireworks flags). The server sends
//! each as an effect ID on the player when the map has loaded; it's a state of the map, drawn as a layer over it until
//! the next map change.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::sprite::Spr;

use crate::render::{Gpu, Texture};
use crate::{Layer, SCREEN};

pub const SNOW: u32 = 162;
pub const SAKURA: u32 = 163;
/// The `clouds` flag (EF_CLOUD3).
pub const CLOUDS: u32 = 233;
pub const FIREWORKS: u32 = 297;
/// Sent with the fireworks; nothing to draw.
pub const FIREWORKS_THROW: u32 = 299;
/// The fireworks' sound, sent with them.
pub const FIREWORKS_SOUND: u32 = 301;
/// The `leaves` flag (EF_MAPLE).
pub const LEAVES: u32 = 333;
/// The `fog` flag (EF_CLOUD4).
pub const FOG: u32 = 515;
/// The `clouds2` flag (EF_CLOUD5).
pub const CLOUDS2: u32 = 516;
/// The item fireworks' effect (itempokjuk, with its sound), which the fireworks weather sets off.
pub const FIREWORK: u32 = 508;

pub fn is_weather(effect: u32) -> bool {
    [SNOW, SAKURA, CLOUDS, FIREWORKS, FIREWORKS_THROW, FIREWORKS_SOUND, LEAVES, FOG, CLOUDS2].contains(&effect)
}

/// The pictures weather is made of; a file missing from the data leaves its weather unseen.
pub struct Assets {
    flakes: Vec<Texture>,
    petals: Vec<Texture>,
    leaves: Vec<Texture>,
    clouds: Vec<Texture>,
    fogs: Vec<Texture>,
}

impl Assets {
    // ponytail: which file is which weather's is from the file names alone; replace with the real client's.
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        // Every picture of a sprite under data\sprite\이팩트\, palette ones then RGBA ones.
        let sprite = |name: &str| {
            let mut textures = Vec::new();
            if let Some(spr) = grf.read(&format!("data\\sprite\\이팩트\\{name}.spr")).ok().and_then(|bytes| Spr::parse(&bytes).ok()) {
                for kind in 0..2 {
                    textures.extend((0..).map_while(|index| spr.image(kind, index, None)).map(|image| gpu.texture(&image, true)));
                }
            }
            textures
        };
        let textures =
            |names: &[String]| names.iter().filter_map(|name| gpu.try_load_texture(grf, &format!("data\\texture\\effect\\{name}"), true).ok()).collect();
        Self {
            flakes: sprite("ef_snow"),
            petals: sprite("sakura01"),
            leaves: sprite("단풍"),
            clouds: textures(&(1..=7).map(|n| format!("cloud{n}.tga")).collect::<Vec<_>>()),
            fogs: textures(&(1..=3).map(|n| format!("fog{n}.tga")).collect::<Vec<_>>()),
        }
    }
}

/// The weather showing: each effect ID and when it began.
#[derive(Default)]
pub struct Weather {
    showing: BTreeMap<u32, Instant>,
    /// When the next firework goes off, and how many have.
    next_firework: Option<Instant>,
    fireworks: u64,
}

impl Weather {
    /// A weather effect from the server; one already showing stays as it is (the server sends all the flags still on
    /// whenever one changes).
    pub fn start(&mut self, effect: u32, now: Instant) {
        self.showing.entry(effect).or_insert(now);
    }

    /// A new map (or the same one again): its weather comes after it has loaded.
    pub fn clear(&mut self) {
        self.showing.clear();
        self.next_firework = None;
    }

    pub fn ids(&self) -> Vec<u32> {
        self.showing.keys().copied().collect()
    }

    // ponytail: a firework every 0.5 to 1.1 s within 4 cells of the player (they burst high above their cell); replace with the real client's timing.
    /// Cells around the player (as offsets) where fireworks go off now.
    pub fn fireworks_due(&mut self, now: Instant) -> Vec<[i32; 2]> {
        if !self.showing.contains_key(&FIREWORKS) {
            return Vec::new();
        }
        let mut due = Vec::new();
        let next = *self.next_firework.get_or_insert(now);
        if now >= next {
            let seed = mix(self.fireworks ^ 0xF1E1);
            self.fireworks += 1;
            due.push([(seed % 9) as i32 - 4, (seed / 9 % 9) as i32 - 4]);
            self.next_firework = Some(now + Duration::from_millis(500 + seed / 81 % 600));
        }
        due
    }

    // ponytail: no data file says how weather moves (how many flakes, how fast, how clouds drift); all of it here is an
    // estimate, drawn over the screen rather than the map, to replace with the real client's.
    pub fn draw<'a>(&self, layer: &mut Layer<'a>, assets: &'a Assets, now: Instant) {
        for (&effect, &since) in &self.showing {
            let seconds = now.saturating_duration_since(since).as_secs_f32();
            match effect {
                SNOW => falling(layer, &assets.flakes, effect, seconds, &Fall { count: 140, speed: [50.0, 110.0], scale: [0.4, 0.9], sway: 15.0, turn: 0.0 }),
                SAKURA => falling(layer, &assets.petals, effect, seconds, &Fall { count: 45, speed: [35.0, 60.0], scale: [0.5, 0.9], sway: 40.0, turn: 4.0 }),
                LEAVES => falling(layer, &assets.leaves, effect, seconds, &Fall { count: 35, speed: [40.0, 70.0], scale: [0.6, 1.0], sway: 50.0, turn: 5.0 }),
                CLOUDS => drifting(layer, &assets.clouds, effect, seconds, &Drift { count: 14, speed: [12.0, 24.0], rise: 0.0, scale: 3.0 }),
                CLOUDS2 => drifting(layer, &assets.clouds, effect, seconds, &Drift { count: 10, speed: [-90.0, -60.0], rise: -12.0, scale: 2.5 }),
                FOG => drifting(layer, &assets.fogs, effect, seconds, &Drift { count: 8, speed: [6.0, 12.0], rise: 0.0, scale: 4.0 }),
                _ => {}
            }
        }
    }
}

/// Pictures falling down the screen, swaying.
struct Fall {
    count: u64,
    /// Pixels a second, the slowest and the fastest.
    speed: [f32; 2],
    /// Of the picture's size.
    scale: [f32; 2],
    /// Pixels to each side.
    sway: f32,
    /// Pictures a second, for those that turn as they fall (the sprite's pictures in order).
    turn: f32,
}

fn falling<'a>(layer: &mut Layer<'a>, images: &'a [Texture], effect: u32, seconds: f32, fall: &Fall) {
    if images.is_empty() {
        return;
    }
    for n in 0..fall.count {
        let [a, b, c, d] = randoms(effect, n);
        let image = &images[((n as f32 + seconds * fall.turn * (0.5 + b)) as usize) % images.len()];
        let scale = between(fall.scale, c);
        let size = [image.size[0] * scale, image.size[1] * scale];
        let room = SCREEN[1] + size[1];
        let y = (a * room + seconds * between(fall.speed, b)).rem_euclid(room) - size[1];
        let x = d * SCREEN[0] + fall.sway * (seconds * (0.5 + c) + d * 6.3).sin();
        layer.sprites.push((image, [x.round(), y.round()], size));
    }
}

/// Large pictures drifting across the screen.
struct Drift {
    count: u64,
    /// Pixels a second sideways (below 0 to the left), the slowest and the fastest.
    speed: [f32; 2],
    /// Pixels a second down (below 0 up).
    rise: f32,
    scale: f32,
}

fn drifting<'a>(layer: &mut Layer<'a>, images: &'a [Texture], effect: u32, seconds: f32, drift: &Drift) {
    if images.is_empty() {
        return;
    }
    for n in 0..drift.count {
        let [a, b, c, _] = randoms(effect, n);
        let image = &images[n as usize % images.len()];
        let size = [image.size[0] * drift.scale, image.size[1] * drift.scale];
        let room = [SCREEN[0] + size[0], SCREEN[1] + size[1]];
        let x = (a * room[0] + seconds * between(drift.speed, b)).rem_euclid(room[0]) - size[0];
        let y = (c * room[1] + seconds * drift.rise).rem_euclid(room[1]) - size[1];
        layer.sprites.push((image, [x.round(), y.round()], size));
    }
}

fn between(range: [f32; 2], fraction: f32) -> f32 {
    range[0] + (range[1] - range[0]) * fraction
}

/// Four fractions in [0, 1) fixed for each particle, so a frame depends only on the time.
fn randoms(effect: u32, n: u64) -> [f32; 4] {
    let seed = mix(u64::from(effect) << 32 | n);
    [0, 16, 32, 48].map(|shift| ((seed >> shift) & 0xFFFF) as f32 / 65536.0)
}

/// splitmix64's finalizer.
fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_one_of_each_until_the_map_changes() {
        let mut weather = Weather::default();
        let now = Instant::now();
        assert!(is_weather(SNOW) && is_weather(FIREWORKS_SOUND) && !is_weather(FIREWORK));
        weather.start(SNOW, now);
        weather.start(FOG, now);
        // The server sends every flag still on again when one changes.
        weather.start(SNOW, now + Duration::from_secs(3));
        assert_eq!(weather.ids(), [SNOW, FOG]);
        assert_eq!(weather.showing[&SNOW], now, "the snow already falling goes on");
        assert!(weather.fireworks_due(now).is_empty());
        weather.start(FIREWORKS, now);
        let first = weather.fireworks_due(now);
        assert!(first.len() == 1 && first[0].iter().all(|cell| cell.abs() <= 4));
        assert!(weather.fireworks_due(now + Duration::from_millis(100)).is_empty(), "not again at once");
        assert_eq!(weather.fireworks_due(now + Duration::from_secs(2)).len(), 1);
        weather.clear();
        assert!(weather.ids().is_empty() && weather.fireworks_due(now + Duration::from_secs(9)).is_empty());
    }

    #[test]
    fn particles_stay_put_for_a_moment() {
        assert_eq!(randoms(SNOW, 4), randoms(SNOW, 4));
        assert_ne!(randoms(SNOW, 4), randoms(SNOW, 5));
        assert!(randoms(FOG, 9).iter().all(|value| (0.0..1.0).contains(value)));
    }
}
