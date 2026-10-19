//! Weather a map has on (rathena's snow, sakura, leaves, clouds, clouds2, fog and fireworks flags). The server sends
//! each as an effect ID on the player when the map has loaded. Every send starts another copy, and they all go at the
//! next map change. Snow, sakura and leaves fall in the world around the player; clouds and fog drift over the screen.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rustnarok_client::grf::Grf;
use rustnarok_client::sprite::{Act, Spr};

use crate::render::{Gpu, Texture};
use crate::world::{Billboard, DepthLayer, EffectQuad, Vector};

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
/// Snow flakes spawned each tick, and how many ticks one lives (640 at once once a copy is that old).
const SNOW_PER_TICK: u32 = 2;
const SNOW_LIFE: u32 = 320;
/// Sakura: one petal on each even tick from 0 through this one (150). Each action has this many frames.
const SAKURA_LAST: u32 = 298;
const SAKURA_FRAMES: u32 = 10;
const SAKURA_ACTIONS: u32 = 3;
/// Leaves: one on each even tick from 0 through this one (100), action 0.
const LEAVES_LAST: u32 = 198;
const LEAVES_FRAMES: u32 = 20;
const PETAL_ALPHA: f32 = 200.0 / 255.0;
const ABOVE: f32 = 200.0;
/// A stalled frame simulates at most this many ticks (10 s). The rest continue on later frames.
const CATCHUP: u32 = 625;

pub fn is_weather(effect: u32) -> bool {
    [SNOW, SAKURA, CLOUDS, FIREWORKS, FIREWORKS_THROW, FIREWORKS_SOUND, LEAVES, FOG, CLOUDS2].contains(&effect)
}

fn is_falling(effect: u32) -> bool {
    matches!(effect, SNOW | SAKURA | LEAVES)
}

/// One sprite's pictures. `actions` is each ACT action's frame image, when the file loaded.
struct Pictures {
    images: Vec<Texture>,
    actions: Vec<Vec<usize>>,
}

impl Pictures {
    fn image(&self, action: usize, frame: usize, span: usize) -> Option<&Texture> {
        if let Some(frames) = self.actions.get(action).filter(|frames| !frames.is_empty()) {
            return self.images.get(frames[frame % frames.len()]);
        }
        // ponytail: no ACT, so action `n` is the next `span` pictures in the sprite (10 sakura, 20 leaves).
        self.images.get(action.saturating_mul(span).saturating_add(frame))
    }
}

/// The pictures weather is made of; a file missing from the data leaves its weather unseen.
pub struct Assets {
    flakes: Pictures,
    petals: Pictures,
    leaves: Pictures,
    /// `pok1.tga`, the fireworks spark. Missing data leaves the sparks unseen.
    spark: Option<Texture>,
    /// `data\etcinfo.txt`: map name to the effects a load starts, and how many of each.
    etc: BTreeMap<String, Vec<(u32, u32)>>,
}

impl Assets {
    // ponytail: which file is which weather's is from the file names alone; replace with the real client's.
    pub fn load(gpu: &Gpu, grf: &Grf) -> Self {
        // Pictures of a sprite under data\sprite\이팩트\, palette ones then RGBA ones, and its ACT frames.
        let sprite = |name: &str| {
            let mut images = Vec::new();
            let mut indexed = 0usize;
            let Some(spr) = grf.read(&format!("data\\sprite\\이팩트\\{name}.spr")).ok().and_then(|bytes| Spr::parse(&bytes).ok()) else {
                return Pictures { images, actions: Vec::new() };
            };
            for kind in 0..2 {
                for index in 0.. {
                    let Some(image) = spr.image(kind, index, None) else { break };
                    if kind == 0 {
                        indexed += 1;
                    }
                    images.push(gpu.texture(&image, true));
                }
            }
            let actions = grf
                .read(&format!("data\\sprite\\이팩트\\{name}.act"))
                .ok()
                .and_then(|bytes| Act::parse(&bytes).ok())
                .map(|act| {
                    act.actions
                        .iter()
                        .map(|frames| {
                            frames
                                .iter()
                                .filter_map(|frame| {
                                    let layer = frame.layers.iter().find(|layer| layer.image >= 0)?;
                                    let image = usize::try_from(layer.image).ok()?;
                                    Some(if layer.kind == 0 { image } else { indexed + image })
                                })
                                .collect()
                        })
                        .collect()
                })
                .unwrap_or_default();
            Pictures { images, actions }
        };
        let text = grf.read("data\\etcinfo.txt").map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
        Self {
            flakes: sprite("ef_snow"),
            petals: sprite("sakura01"),
            leaves: sprite("단풍"),
            spark: gpu.try_load_texture(grf, "data\\texture\\effect\\pok1.tga", true).ok(),
            etc: parse_etcinfo(&text),
        }
    }

    /// Effects `etcinfo.txt` starts for this map, each with its count. Empty when the map is not listed.
    pub fn map_launches(&self, map: &str) -> &[(u32, u32)] {
        self.etc.get(&map_name(map)).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// One flake or petal. Snow falls straight; petals and leaves drift and start again below the player.
enum Flake {
    Snow { at: Vector, age: u32, scale: f32 },
    Drift { at: Vector, age: u32, scale: f32, fall: f32, action: u32, frames: u32, sway: [f32; 2], angle: [i32; 2], target: [i32; 2] },
}

/// One copy of a weather effect. Another packet of the same id is another copy.
struct Running {
    effect: u32,
    start: Instant,
    /// Ticks already simulated. Snow, sakura and leaves only.
    simulated: u32,
    flakes: Vec<Flake>,
}

/// The weather showing: one entry per copy, plus the fireworks sparks and bangs it launched.
#[derive(Default)]
pub struct Weather {
    running: Vec<Running>,
    sparks: Vec<SparkLaunch>,
    bangs: Vec<Bang>,
    seed: u64,
    /// The next redraw starts this map's etcinfo weather, once, if effects are on.
    map_launch: bool,
}

impl Weather {
    /// A weather effect from the server. Each one starts another copy, even of an effect already showing (the server
    /// sends every flag still on whenever one changes). `at` is the player's feet, where fireworks begin.
    pub fn start(&mut self, effect: u32, now: Instant, at: Vector) {
        self.running.push(Running { effect, start: now, simulated: 0, flakes: Vec::new() });
        self.add(effect, 1, now, at);
    }

    /// A map load's etcinfo line. Fireworks add `count` launches and show the id once (Comodo lists 297 once). Snow,
    /// sakura and leaves add `count` copies.
    pub fn launch(&mut self, effect: u32, count: u32, now: Instant, at: Vector) {
        if count == 0 {
            return;
        }
        if is_falling(effect) {
            for _ in 0..count {
                self.running.push(Running { effect, start: now, simulated: 0, flakes: Vec::new() });
            }
        } else if !self.running.iter().any(|running| running.effect == effect) {
            self.running.push(Running { effect, start: now, simulated: 0, flakes: Vec::new() });
        }
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

    /// A new map (or the same one again): every copy goes, and its weather comes after it has loaded.
    pub fn clear(&mut self) {
        self.running.clear();
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

    /// One id per copy, sorted so the control state stays stable.
    pub fn ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.running.iter().map(|running| running.effect).collect();
        ids.sort_unstable();
        ids
    }

    /// How many sparks are running (four per 297).
    #[cfg(test)]
    pub fn spark_count(&self) -> usize {
        self.sparks.len() * SPARKS_PER_LAUNCH as usize
    }

    /// Move each launch onto the player's current feet when its 32 s cycle starts again, and advance falling weather.
    /// `ground_gat` is the ground's GAT height under the player (larger is lower). Flakes keep running with /effect off.
    pub fn step(&mut self, now: Instant, at: Vector, ground_gat: f32) {
        for launch in &mut self.sparks {
            let cycle = ticks_since(launch.start, now) / SPARK_CYCLE;
            if cycle != launch.cycle {
                launch.cycle = cycle;
                launch.origin = west_of(at);
            }
        }
        let mut seed = self.seed;
        for running in &mut self.running {
            if !is_falling(running.effect) {
                continue;
            }
            let due = ticks_since(running.start, now);
            // ponytail: a stalled frame catches up at most 10 s, so a pause doesn't spin; the rest follows next frame.
            let steps = due.saturating_sub(running.simulated).min(CATCHUP);
            for _ in 0..steps {
                seed = tick_running(running, at, ground_gat, seed);
            }
        }
        self.seed = seed;
    }

    /// Simulate `ticks` more 16 ms ticks at `at`. Tests use this so a count doesn't depend on the clock.
    #[cfg(test)]
    fn advance(&mut self, ticks: u32, at: Vector, ground_gat: f32) {
        let mut seed = self.seed;
        for _ in 0..ticks {
            for running in &mut self.running {
                if is_falling(running.effect) {
                    seed = tick_running(running, at, ground_gat, seed);
                }
            }
        }
        self.seed = seed;
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

    /// Snow, sakura and leaves, facing the camera. Empty until their sprites have loaded. Draw only while /effect is on.
    pub fn world_sprites<'a>(&self, assets: &'a Assets) -> Vec<Billboard<'a>> {
        let mut sprites = Vec::new();
        for running in &self.running {
            match running.effect {
                SNOW => {
                    let Some(texture) = assets.flakes.images.first() else { continue };
                    for flake in &running.flakes {
                        let Flake::Snow { at, age, scale } = flake else { continue };
                        sprites.push(centered(texture, *at, *scale, snow_alpha(*age) as f32 / 255.0));
                    }
                }
                SAKURA | LEAVES => {
                    let pictures = if running.effect == SAKURA { &assets.petals } else { &assets.leaves };
                    for flake in &running.flakes {
                        let Flake::Drift { at, age, scale, action, frames, .. } = flake else { continue };
                        let Some(texture) = pictures.image(*action as usize, frame_index(*age, *frames) as usize, *frames as usize) else { continue };
                        sprites.push(centered(texture, *at, *scale, PETAL_ALPHA));
                    }
                }
                _ => {}
            }
        }
        sprites
    }
}

/// A camera-facing sprite centered on `at`. `scale` is a fraction of the picture's size; alpha is 0–1.
fn centered<'a>(texture: &'a Texture, at: Vector, scale: f32, alpha: f32) -> Billboard<'a> {
    let size = [texture.size[0] * scale, texture.size[1] * scale];
    (texture, at, [-size[0] / 2.0, -size[1] / 2.0], size, [1.0, 1.0, 1.0, alpha], 0.0, DepthLayer::default())
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

fn next_rand(seed: &mut u64) -> u32 {
    *seed = seed.wrapping_add(1);
    mix(*seed) as u32
}

/// One tick of a falling copy. `player` is the player's feet; `ground_gat` is the ground under them.
fn tick_running(running: &mut Running, player: Vector, ground_gat: f32, mut seed: u64) -> u64 {
    let tick = running.simulated;
    match running.effect {
        SNOW => {
            for flake in &mut running.flakes {
                let Flake::Snow { at, age, .. } = flake else { continue };
                *age += 1;
                at[1] -= 0.5;
            }
            running.flakes.retain(|flake| matches!(flake, Flake::Snow { age, .. } if *age < SNOW_LIFE));
            let y = snow_world_y(-player[1], ground_gat);
            for _ in 0..SNOW_PER_TICK {
                let theta = next_rand(&mut seed) % 360;
                let radius = 1.0 + (next_rand(&mut seed) % 300) as f32;
                let scale = snow_scale(next_rand(&mut seed));
                let [x, z] = snow_xz([player[0], player[2]], theta, radius);
                running.flakes.push(Flake::Snow { at: [x, y, z], age: 0, scale });
            }
        }
        SAKURA | LEAVES => {
            let leaves = running.effect == LEAVES;
            for flake in &mut running.flakes {
                let Flake::Drift { at, age, fall, sway, angle, target, .. } = flake else { continue };
                at[0] += sway[0] * (angle[0] as f32).to_radians().sin();
                at[2] += sway[1] * (angle[1] as f32).to_radians().sin();
                *age += 1;
                for axis in 0..2 {
                    angle[axis] = chase(angle[axis], target[axis]);
                    if angle[axis] == target[axis] {
                        target[axis] = (next_rand(&mut seed) % 360) as i32;
                    }
                }
                // A restart jumps back up to 200 above the player; a tick that only falls goes down.
                let before = at[1];
                at[1] = petal_y(at[1], *fall, player[1]);
                if at[1] > before {
                    at[0] = beside(player[0], next_rand(&mut seed));
                    at[2] = beside(player[2], next_rand(&mut seed));
                }
            }
            let last = if leaves { LEAVES_LAST } else { SAKURA_LAST };
            if tick <= last && tick.is_multiple_of(2) {
                let scale = hundredths(if leaves { 35 } else { 22 }, next_rand(&mut seed));
                let fall = if leaves { leaf_fall(next_rand(&mut seed)) } else { cherry_fall(next_rand(&mut seed)) };
                let action = if leaves { 0 } else { next_rand(&mut seed) % SAKURA_ACTIONS };
                let frames = if leaves { LEAVES_FRAMES } else { SAKURA_FRAMES };
                let sway = if leaves { [0.12, 0.15] } else { [0.24, 0.30] };
                // ponytail: the spec doesn't give the angle a petal starts at, only the target it chases. It starts at 0.
                let target = [0, 1].map(|_| (next_rand(&mut seed) % 360) as i32);
                running.flakes.push(Flake::Drift {
                    at: [beside(player[0], next_rand(&mut seed)), player[1] + ABOVE, beside(player[2], next_rand(&mut seed))],
                    age: 0,
                    scale,
                    fall,
                    action,
                    frames,
                    sway,
                    angle: [0, 0],
                    target,
                });
            }
        }
        _ => {}
    }
    running.simulated += 1;
    seed
}

/// Snow's alpha at `age` ticks: 0, then +25.4 a tick through 10, held through 256, then down to 0 at 320.
fn snow_alpha(age: u32) -> u32 {
    if age >= SNOW_LIFE {
        0
    } else if age <= 10 {
        age * 254 / 10
    } else if age <= 256 {
        254
    } else {
        254 * (SNOW_LIFE - age) / 64
    }
}

/// World Y (up) where a snow flake starts. `player_gat` and `ground_gat` are GAT heights, larger lower.
fn snow_world_y(player_gat: f32, ground_gat: f32) -> f32 {
    -(player_gat + ground_gat - 100.0)
}

/// A point on the snow disc: `theta` degrees, `radius` world units, x from sin and z from cos.
fn snow_xz([x, z]: [f32; 2], theta: u32, radius: f32) -> [f32; 2] {
    let theta = ((theta % 360) as f32).to_radians();
    [x + radius * theta.sin(), z + radius * theta.cos()]
}

/// (25 + rand % 15) / 50, so 0.50–0.78 of the flake picture.
fn snow_scale(roll: u32) -> f32 {
    (25 + roll % 15) as f32 / 50.0
}

/// An angle chasing `target`: +3° while below it, −2° while above, stopping on it.
fn chase(angle: i32, target: i32) -> i32 {
    if angle < target {
        (angle + 3).min(target)
    } else if angle > target {
        (angle - 2).max(target)
    } else {
        angle
    }
}

/// The picture a petal shows: a new frame every 4 ticks, looping over `frames`.
fn frame_index(age: u32, frames: u32) -> u32 {
    if frames == 0 { 0 } else { (age / 4) % frames }
}

/// Where a petal's height goes this tick. Below the player's feet, it starts again 200 above them.
fn petal_y(y: f32, fall: f32, player_y: f32) -> f32 {
    let next = y - fall;
    if next < player_y { player_y + ABOVE } else { next }
}

// ponytail: the spec gives 0.22–0.27 and 0.35–0.40, not the steps inside. Six hundredths from the low end.
fn hundredths(base: u32, roll: u32) -> f32 {
    (base + roll % 6) as f32 / 100.0
}

fn cherry_fall(roll: u32) -> f32 {
    if roll.is_multiple_of(2) { 0.2 } else { 0.3 }
}

fn leaf_fall(roll: u32) -> f32 {
    [0.06, 0.09, 0.12, 0.15][(roll % 4) as usize]
}

/// Player − 150 … + 149.
fn beside(player: f32, roll: u32) -> f32 {
    player + (roll % 300) as f32 - 150.0
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
    fn copies_add_up_and_clear_on_a_map_load() {
        let mut weather = Weather::default();
        let now = Instant::now();
        let at = [100.0, 0.0, 40.0];
        assert!(is_weather(SNOW) && is_weather(FIREWORKS_SOUND) && !is_weather(FIREWORK));
        weather.start(SNOW, now, at);
        weather.start(FOG, now, at);
        // The server sends every flag still on again when one changes, and each send is another copy.
        weather.start(SNOW, now + Duration::from_secs(3), at);
        weather.start(SAKURA, now, at);
        assert_eq!(weather.ids(), [SNOW, SNOW, SAKURA, FOG]);
        assert_eq!(weather.running[0].start, now, "the first snow keeps the time it started");
        weather.start(FIREWORKS, now, at);
        weather.start(FIREWORKS_THROW, now, at);
        weather.start(FIREWORKS_SOUND, now, at);
        weather.start(FIREWORKS, now, at);
        assert_eq!(weather.spark_count(), SPARKS_PER_LAUNCH as usize * 2, "a resent 297 launches again");
        weather.advance(SNOW_LIFE, at, 0.0);
        assert_eq!(snow_of(&weather).len(), 640 * 2, "each snow copy has its own flakes");
        weather.clear();
        assert!(weather.ids().is_empty() && weather.spark_count() == 0 && snow_of(&weather).is_empty());
        assert_eq!(weather.take_bangs(now + Duration::from_secs(9)), 0);
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
        weather.step(later, [140.0, 6.0, 10.0], 0.0);
        assert!(weather.sparks.iter().all(|launch| launch.origin == [125.0, 6.0, 10.0]));
        assert_eq!(weather.ids(), [FIREWORKS, FIREWORKS_SOUND], "160 launches are still one fireworks id");
    }

    fn fallen(effect: u32, ticks: u32) -> Weather {
        let mut weather = Weather::default();
        weather.start(effect, Instant::now(), [0.0, 0.0, 0.0]);
        weather.advance(ticks, [0.0, 0.0, 0.0], 0.0);
        weather
    }

    fn snow_of(weather: &Weather) -> Vec<&Flake> {
        weather.running.iter().filter(|running| running.effect == SNOW).flat_map(|running| running.flakes.iter()).collect()
    }

    fn drifts_of(weather: &Weather, effect: u32) -> Vec<&Flake> {
        weather.running.iter().filter(|running| running.effect == effect).flat_map(|running| running.flakes.iter()).collect()
    }

    #[test]
    fn snow_sakura_and_leaves_start_on_their_ticks() {
        assert_eq!(snow_of(&fallen(SNOW, 0)).len(), 0);
        assert_eq!(snow_of(&fallen(SNOW, 1)).len(), 2);
        assert_eq!(snow_of(&fallen(SNOW, SNOW_LIFE)).len(), 640, "320 ticks of 2 flakes");
        assert_eq!(snow_of(&fallen(SNOW, SNOW_LIFE + 80)).len(), 640, "the oldest have died and two more began");
        assert_eq!(drifts_of(&fallen(SAKURA, 1), SAKURA).len(), 1);
        assert_eq!(drifts_of(&fallen(SAKURA, 2), SAKURA).len(), 1, "tick 1 is odd");
        assert_eq!(drifts_of(&fallen(SAKURA, 298), SAKURA).len(), 149);
        assert_eq!(drifts_of(&fallen(SAKURA, 299), SAKURA).len(), 150);
        assert_eq!(drifts_of(&fallen(SAKURA, 400), SAKURA).len(), 150);
        assert_eq!(drifts_of(&fallen(LEAVES, 1), LEAVES).len(), 1);
        assert_eq!(drifts_of(&fallen(LEAVES, 199), LEAVES).len(), 100);
        assert_eq!(drifts_of(&fallen(LEAVES, 400), LEAVES).len(), 100);
    }

    #[test]
    fn snow_fades_in_holds_and_fades_out() {
        for (age, alpha) in [(0, 0), (5, 127), (10, 254), (256, 254), (288, 127), (320, 0)] {
            assert_eq!(snow_alpha(age), alpha, "age {age}");
        }
        let weather = fallen(SNOW, 6);
        let oldest = snow_of(&weather).into_iter().map(|flake| match flake {
            Flake::Snow { age, .. } => *age,
            Flake::Drift { .. } => 0,
        });
        assert_eq!(oldest.max(), Some(5));
        assert!(snow_of(&fallen(SNOW, SNOW_LIFE + 1)).iter().all(|flake| match flake {
            Flake::Snow { age, .. } => *age < SNOW_LIFE,
            Flake::Drift { .. } => false,
        }));
    }

    #[test]
    fn snow_starts_on_a_disc_above_flat_ground() {
        assert_eq!(snow_world_y(0.0, 0.0), 100.0, "flat ground at height 0");
        assert_eq!(snow_world_y(10.0, 10.0), 80.0);
        let [x, z] = snow_xz([5.0, 7.0], 0, 1.0);
        assert_eq!((x, z), (5.0, 8.0));
        let [x, z] = snow_xz([0.0, 0.0], 90, 100.0);
        assert!((x - 100.0).abs() < 1e-3 && z.abs() < 1e-3);
        assert_eq!(snow_scale(0), 0.50);
        assert_eq!(snow_scale(14), 0.78);
        let weather = fallen(SNOW, 4);
        for flake in snow_of(&weather) {
            let Flake::Snow { at, scale, .. } = flake else { panic!("snow") };
            let radius = f32::hypot(at[0], at[2]);
            assert!((1.0..=300.0).contains(&radius), "radius {radius}");
            assert!((0.50..=0.78).contains(scale), "scale {scale}");
            assert!((at[1] - 100.0).abs() < 2.0, "still near the start height");
        }
        let mut sloped = Weather::default();
        sloped.start(SNOW, Instant::now(), [0.0, -4.0, 0.0]);
        sloped.advance(1, [0.0, -4.0, 0.0], 4.0);
        let Flake::Snow { at, .. } = snow_of(&sloped)[0] else { panic!("snow") };
        assert!((at[1] - snow_world_y(4.0, 4.0)).abs() < 1e-3);
    }

    #[test]
    fn petals_restart_below_the_player_and_step_their_angles_and_frames() {
        assert!((petal_y(200.0, 0.2, 0.0) - 199.8).abs() < 1e-4);
        assert!((petal_y(0.2, 0.2, 0.0) - 0.0).abs() < 1e-4, "level with the player is not below");
        assert!((petal_y(0.2, 0.3, 0.0) - 200.0).abs() < 1e-4, "below the player starts again 200 up");
        assert_eq!(chase(0, 10), 3);
        assert_eq!(chase(8, 10), 10);
        assert_eq!(chase(10, 0), 8);
        assert_eq!(chase(1, 0), 0);
        assert_eq!(chase(4, 4), 4);
        assert_eq!(frame_index(0, 10), 0);
        assert_eq!(frame_index(3, 10), 0);
        assert_eq!(frame_index(4, 10), 1);
        assert_eq!(frame_index(40, 10), 0);
        assert_eq!(frame_index(4, 20), 1);
        assert_eq!(frame_index(80, 20), 0);
        assert_eq!(hundredths(22, 0), 0.22);
        assert_eq!(hundredths(22, 5), 0.27);
        assert_eq!(hundredths(35, 5), 0.40);
        assert_eq!(cherry_fall(0), 0.2);
        assert_eq!(cherry_fall(1), 0.3);
        assert_eq!(leaf_fall(3), 0.15);
        assert_eq!(beside(10.0, 0), -140.0);
        assert_eq!(beside(10.0, 299), 159.0);
        let sakura = fallen(SAKURA, 5);
        let oldest = drifts_of(&sakura, SAKURA)
            .into_iter()
            .filter_map(|flake| match flake {
                Flake::Drift { age, frames, at, angle, target, scale, fall, .. } => Some((*age, *frames, *at, *angle, *target, *scale, *fall)),
                Flake::Snow { .. } => None,
            })
            .max_by_key(|flake| flake.0)
            .unwrap();
        assert_eq!(oldest.0, 4, "spawned on tick 0, stepped four times");
        assert_eq!(frame_index(oldest.0, oldest.1), 1);
        assert!((0.22..=0.27).contains(&oldest.5));
        assert!(oldest.6 == 0.2 || oldest.6 == 0.3);
        assert!(oldest.3.iter().all(|angle| (0..=359).contains(angle)));
        assert!(oldest.4.iter().all(|angle| (0..=359).contains(angle)));
        // 0.2 a tick needs 1001 steps to go below 0; 0.3 needs fewer. 1200 covers either, and none stay below.
        let sakura = fallen(SAKURA, 1200);
        let leaves = fallen(LEAVES, 4000);
        for flake in drifts_of(&sakura, SAKURA).into_iter().chain(drifts_of(&leaves, LEAVES)) {
            let Flake::Drift { at, scale, fall, .. } = flake else { panic!("drift") };
            assert!(at[1] >= 0.0, "restarted once it was below the player");
            let (low, high) = if *fall <= 0.15 { (0.35, 0.40) } else { (0.22, 0.27) };
            assert!((low..=high).contains(scale));
        }
        assert_eq!(drifts_of(&sakura, SAKURA).len(), 150);
        assert_eq!(drifts_of(&leaves, LEAVES).len(), 100);
    }
}
