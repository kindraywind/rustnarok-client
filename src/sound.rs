//! Sound: each map's background music (`BGM\<n>.mp3` in the client's folder, as data.grf's mp3nametable.txt names
//! it) and the effects in data.grf (`data\wav\`), decoded with symphonia and mixed into the default output with cpal.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustnarok_client::grf::Grf;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decoded sound: interleaved 16-bit stereo at its own rate (the mixer resamples as it plays, which keeps a song
/// a third the size of float samples at the output's rate).
struct Sound {
    samples: Vec<i16>,
    rate: u32,
}

type Samples = Arc<Sound>;

/// How many effects play at once; the oldest stops for a new one.
const MOST_EFFECTS: usize = 24;
/// The loudest a volume goes (as /bv and /v take it).
pub const MAX_VOLUME: u8 = 127;

/// The player's sound settings: music and effects each on or off, with a volume of 0 to `MAX_VOLUME`; kept with the
/// other settings (`settings_path`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub on: [bool; 2],
    pub volume: [u8; 2],
}

impl Default for Settings {
    fn default() -> Self {
        Self { on: [true; 2], volume: [100; 2] }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        crate::settings_path("sound")
    }

    pub fn load() -> Self {
        let text = Self::path().and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default();
        let numbers: Vec<u8> = text.split_whitespace().filter_map(|number| number.parse().ok()).collect();
        match numbers[..] {
            [music_on, music, effects_on, effects] => {
                Self { on: [music_on != 0, effects_on != 0], volume: [music, effects].map(|volume| volume.min(MAX_VOLUME)) }
            }
            _ => Self::default(),
        }
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        let text = format!("{} {} {} {}", u8::from(self.on[0]), self.volume[0], u8::from(self.on[1]), self.volume[1]);
        let _ = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, text));
    }

    /// The gains of music and effects.
    fn gains(&self) -> [f32; 2] {
        [0, 1].map(|kind| if self.on[kind] { f32::from(self.volume[kind]) / f32::from(MAX_VOLUME) } else { 0.0 })
    }

    /// The chat's sound commands: /music and /sound turn music or effects on and off, /bv and /v (0 to 127) set
    /// their volumes. None for any other text; otherwise the message (by number) to show, if any.
    pub fn command(&mut self, text: &str) -> Option<Option<usize>> {
        let (command, value) = text.split_once(' ').map_or((text, None), |(command, value)| (command, Some(value.trim())));
        let kind = match command {
            "/music" | "/bv" => 0,
            "/sound" | "/v" => 1,
            _ => return None,
        };
        if command == "/bv" || command == "/v" {
            if let Some(volume) = value.and_then(|value| value.parse::<u32>().ok()) {
                self.volume[kind] = volume.min(u32::from(MAX_VOLUME)) as u8;
            }
            return Some(None);
        }
        self.on[kind] = !self.on[kind];
        // "BGM On", "BGM Off", "Sound Effects On", "Sound Effects Off".
        Some(Some(match (kind, self.on[kind]) {
            (0, true) => 31,
            (0, false) => 32,
            (_, true) => 27,
            (_, false) => 28,
        }))
    }
}

struct Voice {
    sound: Samples,
    /// Where it's at, in its own frames, and how far one output frame moves that.
    at: f64,
    step: f64,
    volume: f32,
    music: bool,
}

struct Mixer {
    voices: Vec<Voice>,
    /// Music and effect volumes, 0 to 1.
    volumes: [f32; 2],
    /// The music wanted now: a file still decoding only starts if it's still wanted when done.
    music: Option<PathBuf>,
}

impl Mixer {
    /// Fills an output buffer of `channels` interleaved channels.
    fn mix(&mut self, data: &mut [f32], channels: usize) {
        data.fill(0.0);
        let volumes = self.volumes;
        for voice in &mut self.voices {
            let gain = voice.volume * volumes[usize::from(!voice.music)] / 32_768.0;
            let (samples, frames) = (&voice.sound.samples, voice.sound.samples.len() / 2);
            for frame in data.chunks_mut(channels) {
                // Between two of its frames, linearly; music goes round, an effect stops at its end.
                let mut first = voice.at as usize;
                if first >= frames {
                    if !voice.music {
                        break;
                    }
                    voice.at = voice.at.fract();
                    first = 0;
                }
                let next = if voice.music { (first + 1) % frames } else { (first + 1).min(frames - 1) };
                let part = voice.at.fract() as f32;
                for (channel, out) in frame.iter_mut().take(2).enumerate() {
                    let (from, to) = (f32::from(samples[first * 2 + channel]), f32::from(samples[next * 2 + channel]));
                    *out += (from + (to - from) * part) * gain;
                }
                voice.at += voice.step;
            }
        }
        self.voices.retain(|voice| voice.music || (voice.at as usize) < voice.sound.samples.len() / 2);
        for sample in data {
            *sample = sample.clamp(-1.0, 1.0);
        }
    }
}

pub struct Audio {
    mixer: Arc<Mutex<Mixer>>,
    rate: u32,
    _stream: cpal::Stream,
    effects: HashMap<String, Option<Samples>>,
}

impl Audio {
    /// The default output device, or None when there's none (or `RUSTNAROK_CLIENT_SOUND=0`).
    pub fn new(settings: &Settings) -> Option<Self> {
        if std::env::var("RUSTNAROK_CLIENT_SOUND").is_ok_and(|sound| sound == "0") {
            return None;
        }
        let device = cpal::default_host().default_output_device()?;
        let config = device.default_output_config().ok()?;
        // ponytail: f32 output only, which is what Core Audio gives.
        if config.sample_format() != cpal::SampleFormat::F32 {
            return None;
        }
        let mixer = Arc::new(Mutex::new(Mixer { voices: Vec::new(), volumes: settings.gains(), music: None }));
        let (channels, rate) = (usize::from(config.channels()), config.sample_rate());
        let output = mixer.clone();
        let stream = device
            .build_output_stream(
                &config.config(),
                move |data: &mut [f32], _| output.lock().unwrap().mix(data, channels),
                |error| eprintln!("sound: {error}"),
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some(Self { mixer, rate, _stream: stream, effects: HashMap::new() })
    }

    pub fn set(&mut self, settings: &Settings) {
        self.mixer.lock().unwrap().volumes = settings.gains();
    }

    /// Plays music from a file on a loop (decoding it on a thread), unless it's what plays already; None stops it.
    pub fn play_music(&mut self, path: Option<&Path>) {
        let mut mixer = self.mixer.lock().unwrap();
        if mixer.music.as_deref() == path {
            return;
        }
        mixer.music = path.map(Path::to_owned);
        mixer.voices.retain(|voice| !voice.music);
        let Some(path) = path.map(Path::to_owned) else { return };
        let (mixer, rate) = (self.mixer.clone(), self.rate);
        std::thread::spawn(move || {
            let sound = std::fs::read(&path).ok().and_then(|bytes| decode(bytes, "mp3"));
            let mut mixer = mixer.lock().unwrap();
            if let Some(sound) = sound.filter(|_| mixer.music.as_ref() == Some(&path)) {
                mixer.voices.push(Voice::new(sound, rate, 1.0, true));
            }
        });
    }

    /// The music playing (or loading), as a path.
    pub fn music(&self) -> Option<PathBuf> {
        self.mixer.lock().unwrap().music.clone()
    }

    /// Plays an effect from data.grf (`data\wav\<name>`) at `volume`, 0 to 1.
    pub fn play(&mut self, grf: &Grf, name: &str, volume: f32) {
        if volume <= 0.0 {
            return;
        }
        let sound = self.effects.entry(name.to_lowercase()).or_insert_with(|| decode(grf.read(&format!("data\\wav\\{name}")).ok()?, "wav")).clone();
        let Some(sound) = sound else { return };
        let mut mixer = self.mixer.lock().unwrap();
        let effects = mixer.voices.iter().filter(|voice| !voice.music).count();
        if effects >= MOST_EFFECTS
            && let Some(oldest) = mixer.voices.iter().position(|voice| !voice.music)
        {
            mixer.voices.remove(oldest);
        }
        mixer.voices.push(Voice::new(sound, self.rate, volume, false));
    }
}

impl Voice {
    fn new(sound: Samples, rate: u32, volume: f32, music: bool) -> Self {
        let step = f64::from(sound.rate) / f64::from(rate);
        Self { sound, at: 0.0, step, volume, music }
    }
}

/// Decodes a WAV or MP3.
fn decode(bytes: Vec<u8>, extension: &str) -> Option<Samples> {
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(extension);
    let mut format = symphonia::default::get_probe().format(&hint, source, &FormatOptions::default(), &MetadataOptions::default()).ok()?.format;
    let track = format.default_track()?;
    let (track_id, source_rate) = (track.id, track.codec_params.sample_rate?);
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).ok()?;
    let mut samples = Vec::with_capacity(track.codec_params.n_frames.unwrap_or(0) as usize * 2);
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let Ok(decoded) = decoder.decode(&packet) else { continue };
        let spec = *decoded.spec();
        let mut buffer = SampleBuffer::<i16>::new(decoded.capacity() as u64, spec);
        buffer.copy_interleaved_ref(decoded);
        let channels = spec.channels.count().max(1);
        for frame in buffer.samples().chunks_exact(channels) {
            samples.extend([frame[0], frame[channels.min(2) - 1]]);
        }
    }
    (samples.len() >= 4).then(|| Arc::new(Sound { samples, rate: source_rate }))
}

/// The music each map plays (lowercase map name without extension → file), from data.grf's mp3nametable.txt
/// (`prontera.rsw#bgm\\08.mp3#`).
pub fn music_table(grf: &Grf) -> HashMap<String, String> {
    let Ok(bytes) = grf.read("data\\mp3nametable.txt") else { return HashMap::new() };
    let text = encoding_rs::EUC_KR.decode(&bytes).0;
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('#');
            let map = fields.next()?.trim().to_lowercase();
            let file = fields.next()?.trim().replace("\\\\", "\\");
            (!map.starts_with("//") && !file.is_empty()).then(|| (map.trim_end_matches(".rsw").to_owned(), file))
        })
        .collect()
}

/// Where a music file named in mp3nametable.txt (`bgm\08.mp3`) is: `RUSTNAROK_CLIENT_BGM` (a folder) or `BGM` beside the client.
pub fn music_path(file: &str) -> PathBuf {
    let folder = std::env::var("RUSTNAROK_CLIENT_BGM").unwrap_or_else(|_| "BGM".into());
    let name = file.rsplit(['\\', '/']).next().unwrap_or(file);
    // Scripts may leave the extension off (rathena's playBGM "99").
    let name = if name.contains('.') { name.to_owned() } else { format!("{name}.mp3") };
    Path::new(&folder).join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_and_resamples() {
        let sound = |samples: Vec<i16>| Arc::new(Sound { samples, rate: 1 });
        let voice = Voice::new(sound(vec![16_384, -16_384, 24_576, 8_192]), 1, 1.0, false);
        let mut mixer = Mixer { voices: vec![voice], volumes: [1.0, 0.5], music: None };
        let mut out = [0.0; 6];
        mixer.mix(&mut out, 2);
        assert_eq!(out, [0.25, -0.25, 0.375, 0.125, 0.0, 0.0]);
        assert!(mixer.voices.is_empty(), "a finished effect goes");
        // Twice the rate out: every other frame halfway between two.
        mixer.voices.push(Voice::new(sound(vec![0, 0, 16_384, 16_384]), 2, 1.0, true));
        let mut out = [0.0; 5];
        mixer.mix(&mut out, 1);
        assert_eq!(out, [0.0, 0.25, 0.5, 0.25, 0.0], "music goes round");
    }

    #[test]
    fn names_music_files() {
        let name = |file: &str| music_path(file).file_name().map(|name| name.to_string_lossy().into_owned());
        assert_eq!(name("bgm\\01.mp3").as_deref(), Some("01.mp3"));
        assert_eq!(name("99").as_deref(), Some("99.mp3"));
    }

    #[test]
    fn takes_the_chats_sound_commands() {
        let mut settings = Settings::default();
        assert_eq!(settings.command("/music"), Some(Some(32)));
        assert_eq!(settings.command("/v 300"), Some(None));
        assert_eq!(settings.command("/bv x"), Some(None));
        assert_eq!(settings.command("/sit"), None);
        assert_eq!(settings, Settings { on: [false, true], volume: [100, MAX_VOLUME] });
        assert_eq!(settings.gains(), [0.0, 1.0]);
    }

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf` (and the music in `BGM`, if it's there).
    #[test]
    fn decodes_the_clients_sounds() {
        let Some(grf) = Grf::for_tests() else { return };
        let attack = decode(grf.read("data\\wav\\poring_attack.wav").unwrap(), "wav").unwrap();
        assert!(attack.samples.len() > 2_000, "{}", attack.samples.len());
        let music = music_table(&grf);
        assert_eq!(music.get("prontera").map(String::as_str), Some("bgm\\08.mp3"));
        let path = music_path(&music["prontera"]);
        if let Ok(bytes) = std::fs::read(&path) {
            let theme = decode(bytes, "mp3").unwrap();
            assert!(theme.samples.len() > theme.rate as usize * 2 * 30, "{path:?} lasts over 30 s");
        }
    }
}
