//! Weather a map has on (rathena's snow, sakura, leaves, clouds, clouds2, fog and fireworks flags). The server sends
//! each as an effect ID on the player when the map has loaded; it's a state of the map, drawn as a layer over it until
//! the next map change.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::sprite::Spr;

use crate::render::{Gpu, Texture};
use crate::world::{EffectQuad, Vector};
use crate::{Layer, screen};

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
/// The item fireworks' effect. The fireworks flag does not play it.
#[cfg(test)]
const FIREWORK: u32 = 508;
/// One launch of effect 297 is this many sparks.
pub const SPARKS_PER_LAUNCH: u32 = 4;
/// Comodo's etcinfo line starts this many launches.
pub const COMODO_LAUNCHES: u32 = 160;
/// A spark waits, rises unseen, bursts, and starts again after this many 16 ms ticks.
pub const SPARK_CYCLE: u32 = 2000;
/// The bang plays at this tick, then every `BANG_PERIOD` ticks.
pub const BANG_FIRST: u32 = 300;
pub const BANG_PERIOD: u32 = 2002;
pub const BANG_SOUND: &str = "effect\\폭죽.wav";
const TICK: Duration = Duration::from_millis(16);
const RISE_TICKS: u32 = 154;
const RISE_STEP: f32 = 0.25;
const SPARK_ALPHA: i32 = 250;
const WEST: f32 = 15.0;

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
    /// `pok1.tga`, the fireworks spark. Missing data leaves the sparks unseen.
    spark: Option<Texture>,
    /// `data\etcinfo.txt`: map name to the effects a load starts, and how many of each.
    etc: BTreeMap<String, Vec<(u32, u32)>>,
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
        let text = grf.read("data\\etcinfo.txt").map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
        Self {
            flakes: sprite("ef_snow"),
            petals: sprite("sakura01"),
            leaves: sprite("단풍"),
            clouds: textures(&(1..=7).map(|n| format!("cloud{n}.tga")).collect::<Vec<_>>()),
            fogs: textures(&(1..=3).map(|n| format!("fog{n}.tga")).collect::<Vec<_>>()),
            spark: gpu.try_load_texture(grf, "data\\texture\\effect\\pok1.tga", true).ok(),
            etc: parse_etcinfo(&text),
        }
    }

    /// Effects `etcinfo.txt` starts for this map, each with its count. Empty when the map is not listed.
    pub fn map_launches(&self, map: &str) -> &[(u32, u32)] {
        self.etc.get(&map_name(map)).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// The weather showing: each effect ID and when it began, plus the fireworks sparks and bangs it launched.
#[derive(Default)]
pub struct Weather {
    showing: BTreeMap<u32, Instant>,
    sparks: Vec<SparkLaunch>,
    bangs: Vec<Bang>,
    seed: u64,
    /// The next redraw starts this map's etcinfo weather, once, if effects are on.
    map_launch: bool,
}

impl Weather {
    /// A weather effect from the server; one already showing stays as it is (the server sends all the flags still on
    /// whenever one changes). `at` is the player's feet, where fireworks begin.
    pub fn start(&mut self, effect: u32, now: Instant, at: Vector) {
        if self.showing.contains_key(&effect) {
            return;
        }
        self.showing.insert(effect, now);
        self.add(effect, 1, now, at);
    }

    /// A map load's etcinfo line: `count` copies, even of an effect the load already started.
    pub fn launch(&mut self, effect: u32, count: u32, now: Instant, at: Vector) {
        if count == 0 {
            return;
        }
        self.showing.entry(effect).or_insert(now);
        self.add(effect, count, now, at);
    }

    fn add(&mut self, effect: u32, count: u32, now: Instant, at: Vector) {
        let origin = west_of(at);
        match effect {
            FIREWORKS => {
                for _ in 0..count {
                    let sparks = std::array::from_fn(|_| {
                        self.seed = self.seed.wrapping_add(1);
                        Spark::from_seed(self.seed)
                    });
                    self.sparks.push(SparkLaunch { start: now, sparks, origin, cycle: 0 });
                }
            }
            FIREWORKS_SOUND => {
                self.bangs.extend((0..count).map(|_| Bang { start: now, heard: 0 }));
            }
            _ => {}
        }
    }

    /// A new map (or the same one again): its weather comes after it has loaded.
    pub fn clear(&mut self) {
        self.showing.clear();
        self.sparks.clear();
        self.bangs.clear();
        self.map_launch = true;
    }

    /// The map just loaded (the first one too): etcinfo runs on the next redraw.
    pub fn begin_map(&mut self) {
        self.map_launch = true;
    }

    /// True once after a map load. Effects off still consumes it, so nothing starts later.
    pub fn take_map_launch(&mut self) -> bool {
        let due = self.map_launch;
        self.map_launch = false;
        due
    }

    pub fn ids(&self) -> Vec<u32> {
        self.showing.keys().copied().collect()
    }

    /// How many sparks are running (four per 297).
    #[cfg(test)]
    pub fn spark_count(&self) -> usize {
        self.sparks.len() * SPARKS_PER_LAUNCH as usize
    }

    /// Move each launch onto the player's current feet when its 32 s cycle starts again.
    pub fn step(&mut self, now: Instant, at: Vector) {
        for launch in &mut self.sparks {
            let cycle = ticks_since(launch.start, now) / SPARK_CYCLE;
            if cycle != launch.cycle {
                launch.cycle = cycle;
                launch.origin = west_of(at);
            }
        }
    }

    /// Bangs whose tick has arrived since last time. They are marked heard either way.
    pub fn take_bangs(&mut self, now: Instant) -> usize {
        self.bangs.iter_mut().filter_map(|bang| bang.take(now).then_some(())).count()
    }

    /// Spark squares and their trails, depth-tested by the effect pass. Empty until `pok1.tga` has loaded.
    pub fn spark_quads<'a>(&self, assets: &'a Assets, now: Instant) -> Vec<EffectQuad<'a>> {
        let Some(texture) = &assets.spark else { return Vec::new() };
        let mut quads = Vec::new();
        for launch in &self.sparks {
            let age = ticks_since(launch.start, now);
            for spark in &launch.sparks {
                let frame = spark_frame(spark, age, launch.origin);
                if !frame.visible {
                    continue;
                }
                let color = |alpha: i32| {
                    let [r, g, b] = spark.color.map(|channel| f32::from(channel) / 255.0);
                    [r, g, b, alpha as f32 / 255.0]
                };
                quads.push((texture, frame.position, square(2.0), color(frame.alpha), true, false));
                if frame.trail_alpha > 0 {
                    quads.push((texture, frame.trail_position, square(1.5), color(frame.trail_alpha), true, false));
                }
            }
        }
        quads
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
        let room = screen()[1] + size[1];
        let y = (a * room + seconds * between(fall.speed, b)).rem_euclid(room) - size[1];
        let x = d * screen()[0] + fall.sway * (seconds * (0.5 + c) + d * 6.3).sin();
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
        let room = [screen()[0] + size[0], screen()[1] + size[1]];
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

/// `data\etcinfo.txt`: `weather#<map>.rsw#<kind>#`, skipping `//` lines.
pub fn parse_etcinfo(text: &str) -> BTreeMap<String, Vec<(u32, u32)>> {
    let mut maps: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let mut parts = line.split('#');
        if parts.next() != Some("weather") {
            continue;
        }
        let (Some(file), Some(kind)) = (parts.next(), parts.next()) else { continue };
        let launches = match kind {
            "snow" => vec![(SNOW, 1)],
            "sakura" => vec![(SAKURA, 1)],
            "pokjuk" => vec![(FIREWORKS_SOUND, 1), (FIREWORKS, COMODO_LAUNCHES)],
            _ => continue,
        };
        maps.entry(map_name(file)).or_default().extend(launches);
    }
    maps
}

fn map_name(map: &str) -> String {
    map.trim_end_matches(".gat").trim_end_matches(".rsw").to_lowercase()
}

/// 15 units west of the player's feet.
fn west_of(at: Vector) -> Vector {
    [at[0] - WEST, at[1], at[2]]
}

fn ticks_since(start: Instant, now: Instant) -> u32 {
    (now.saturating_duration_since(start).as_millis() / TICK.as_millis()) as u32
}

/// One spark of a 297 launch. Its numbers are fixed when it starts.
#[derive(Clone, Copy)]
pub struct Spark {
    pub wait: u32,
    pub pitch: f32,
    pub yaw: f32,
    pub speed: f32,
    pub fade: u32,
    pub color: [u8; 3],
}

const SPARK_COLORS: [[u8; 3]; 5] = [[0x46, 0x46, 0xFF], [0xFF, 0x46, 0x46], [0x46, 0xFF, 0x46], [0xFF, 0xFF, 0x5A], [0xFF, 0x46, 0xFF]];

impl Spark {
    fn from_seed(seed: u64) -> Self {
        Self {
            wait: 300 + (mix(seed) % 50) as u32,
            pitch: (mix(seed ^ 1) % 360) as f32,
            yaw: (mix(seed ^ 2) % 360) as f32,
            speed: 0.4 + (mix(seed ^ 3) % 401) as f32 / 1000.0,
            fade: if mix(seed ^ 4).is_multiple_of(2) { 1 } else { 2 },
            color: SPARK_COLORS[(mix(seed ^ 5) % 5) as usize],
        }
    }
}

struct SparkLaunch {
    start: Instant,
    sparks: [Spark; SPARKS_PER_LAUNCH as usize],
    origin: Vector,
    cycle: u32,
}

struct Bang {
    start: Instant,
    heard: u32,
}

impl Bang {
    fn take(&mut self, now: Instant) -> bool {
        let age = ticks_since(self.start, now);
        let from = self.heard;
        self.heard = age;
        if age < BANG_FIRST || from >= age {
            return false;
        }
        let first = from.max(BANG_FIRST - 1) + 1;
        let tick = BANG_FIRST + BANG_PERIOD * first.saturating_sub(BANG_FIRST).div_ceil(BANG_PERIOD);
        tick <= age
    }
}

/// Where a spark is on tick `age` of its cycle, measured from `origin` (the point 15 units west of the player).
pub struct SparkFrame {
    pub visible: bool,
    pub position: Vector,
    pub alpha: i32,
    pub trail_position: Vector,
    pub trail_alpha: i32,
}

pub fn spark_frame(spark: &Spark, age: u32, origin: Vector) -> SparkFrame {
    let phase = age % SPARK_CYCLE;
    let burst_at = spark.wait + RISE_TICKS;
    if phase < burst_at {
        let risen = if phase < spark.wait { 0.0 } else { RISE_STEP * (phase - spark.wait) as f32 };
        let position = [origin[0], origin[1] + risen, origin[2]];
        return SparkFrame { visible: false, position, alpha: 0, trail_position: position, trail_alpha: 0 };
    }
    let burst = phase - burst_at;
    let dir = direction(spark.pitch, spark.yaw);
    let mut position = [origin[0], origin[1] + RISE_STEP * RISE_TICKS as f32, origin[2]];
    let mut velocity = dir.map(|axis| axis * spark.speed);
    let mut trail = position;
    for step in 0..burst {
        position[0] += velocity[0];
        position[1] += velocity[1] - 0.02;
        position[2] += velocity[2];
        velocity = velocity.map(|axis| axis * 0.98);
        if (step + 1).is_multiple_of(3) {
            trail = position;
        }
    }
    let alpha = SPARK_ALPHA - spark.fade as i32 * burst as i32;
    let visible = alpha > 0;
    SparkFrame { visible, position, alpha: alpha.max(0), trail_position: trail, trail_alpha: if visible { alpha / 2 } else { 0 } }
}

fn direction(pitch: f32, yaw: f32) -> Vector {
    let (pitch, yaw) = (pitch.to_radians(), yaw.to_radians());
    [pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos()]
}

/// How far a burst's speed carries it: `speed` the first tick, × 0.98 each tick after. 0.4 lands on 20, 0.8 on 40.
#[cfg(test)]
fn burst_reach(speed: f32) -> f32 {
    speed * (1.0 - 0.98_f32.powi(2000)) / 0.02
}

/// The bang's ticks: 300, then every 2002.
#[cfg(test)]
fn bang_due(age: u32) -> bool {
    age >= BANG_FIRST && (age - BANG_FIRST).is_multiple_of(BANG_PERIOD)
}

/// A camera-facing square whose half-diagonal is `units` world units, in sprite pixels (y down).
fn square(units: f32) -> [[f32; 2]; 4] {
    let half = units / (crate::world::CELL / 35.0) / std::f32::consts::SQRT_2;
    [[-half, -half], [half, -half], [half, half], [-half, half]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_one_of_each_until_the_map_changes() {
        let mut weather = Weather::default();
        let now = Instant::now();
        let at = [100.0, 0.0, 40.0];
        assert!(is_weather(SNOW) && is_weather(FIREWORKS_SOUND) && !is_weather(FIREWORK));
        weather.start(SNOW, now, at);
        weather.start(FOG, now, at);
        // The server sends every flag still on again when one changes.
        weather.start(SNOW, now + Duration::from_secs(3), at);
        assert_eq!(weather.ids(), [SNOW, FOG]);
        assert_eq!(weather.showing[&SNOW], now, "the snow already falling goes on");
        weather.start(FIREWORKS, now, at);
        weather.start(FIREWORKS_THROW, now, at);
        weather.start(FIREWORKS_SOUND, now, at);
        assert_eq!(weather.spark_count(), SPARKS_PER_LAUNCH as usize);
        assert_eq!(weather.ids(), [SNOW, FIREWORKS, FIREWORKS_THROW, FIREWORKS_SOUND, FOG]);
        // A resent flag does not add another launch.
        weather.start(FIREWORKS, now, at);
        assert_eq!(weather.spark_count(), SPARKS_PER_LAUNCH as usize);
        weather.clear();
        assert!(weather.ids().is_empty() && weather.spark_count() == 0 && weather.take_bangs(now + Duration::from_secs(9)) == 0);
    }

    #[test]
    fn etcinfo_lists_comodo_and_skips_comments() {
        let text = "\
//weather#pay_arche.rsw#snow#
weather#comodo.rsw#pokjuk#
weather#prontera.rsw#snow#
weather#geffen.gat#sakura#
weather#other.rsw#rain#
";
        let maps = parse_etcinfo(text);
        assert!(!maps.contains_key("pay_arche"), "a comment line");
        assert!(!maps.contains_key("izlude"), "a map not listed");
        assert_eq!(maps["prontera"], [(SNOW, 1)]);
        assert_eq!(maps["geffen"], [(SAKURA, 1)]);
        assert!(!maps.contains_key("other"), "a kind with nothing to launch");
        assert_eq!(maps["comodo"], [(FIREWORKS_SOUND, 1), (FIREWORKS, COMODO_LAUNCHES)]);
    }

    #[test]
    fn a_spark_waits_rises_bursts_and_restarts() {
        let spark = Spark { wait: 300, pitch: 90.0, yaw: 0.0, speed: 0.4, fade: 1, color: SPARK_COLORS[0] };
        let origin = [0.0, 1.0, 0.0];
        let waiting = spark_frame(&spark, 299, origin);
        assert!(!waiting.visible && waiting.position == origin);
        let rising = spark_frame(&spark, 300 + 154 - 1, origin);
        assert!(!rising.visible);
        assert!((rising.position[1] - (origin[1] + 0.25 * 153.0)).abs() < 1e-3);
        let burst = spark_frame(&spark, 300 + 154, origin);
        assert!(burst.visible && burst.alpha == 250);
        assert!((burst.position[1] - (origin[1] + 38.5)).abs() < 1e-3, "154 rises of 0.25");
        let faded = spark_frame(&spark, 300 + 154 + 100, origin);
        assert_eq!(faded.alpha, 150);
        let gone = spark_frame(&spark, 300 + 154 + 250, origin);
        assert!(!gone.visible && gone.alpha == 0);
        let restarted = spark_frame(&spark, SPARK_CYCLE, [10.0, 4.0, 2.0]);
        assert!(!restarted.visible && restarted.position == [10.0, 4.0, 2.0]);
    }

    #[test]
    fn the_burst_carries_between_20_and_40() {
        assert!((burst_reach(0.4) - 20.0).abs() < 1e-2);
        assert!((burst_reach(0.8) - 40.0).abs() < 1e-2);
        let mid = burst_reach(0.6);
        assert!(mid > 20.0 && mid < 40.0);
    }

    #[test]
    fn the_bang_is_at_300_then_every_2002_ticks() {
        assert!(!bang_due(0) && !bang_due(299));
        assert!(bang_due(BANG_FIRST));
        assert!(!bang_due(BANG_FIRST + 1));
        assert!(bang_due(BANG_FIRST + BANG_PERIOD));
        let mut weather = Weather::default();
        let start = Instant::now();
        weather.launch(FIREWORKS_SOUND, 1, start, [0.0; 3]);
        assert_eq!(weather.take_bangs(start + Duration::from_millis(16 * 299)), 0);
        assert_eq!(weather.take_bangs(start + Duration::from_millis(16 * 300)), 1);
        assert_eq!(weather.take_bangs(start + Duration::from_millis(16 * 301)), 0);
        assert_eq!(weather.take_bangs(start + Duration::from_millis(16 * (300 + 2002))), 1);
    }

    #[test]
    fn comodo_starts_640_sparks_fifteen_west_and_follows_later() {
        let mut weather = Weather::default();
        let start = Instant::now();
        let at = [100.0, 2.0, 30.0];
        weather.launch(FIREWORKS, COMODO_LAUNCHES, start, at);
        weather.launch(FIREWORKS_SOUND, 1, start, at);
        assert_eq!(weather.spark_count(), 640);
        assert_eq!(weather.bangs.len(), 1);
        assert!(weather.sparks.iter().all(|launch| launch.origin == [85.0, 2.0, 30.0]));
        let later = start + Duration::from_millis(16 * u64::from(SPARK_CYCLE));
        weather.step(later, [140.0, 6.0, 10.0]);
        assert!(weather.sparks.iter().all(|launch| launch.origin == [125.0, 6.0, 10.0]));
    }

    #[test]
    fn particles_stay_put_for_a_moment() {
        assert_eq!(randoms(SNOW, 4), randoms(SNOW, 4));
        assert_ne!(randoms(SNOW, 4), randoms(SNOW, 5));
        assert!(randoms(FOG, 9).iter().all(|value| (0.0..1.0).contains(value)));
    }
}
