//! The client's Lua data tables (compiled Lua 5.1 in data.grf), run in an embedded Lua 5.1.

use std::collections::HashMap;
use std::io;

use mlua::Lua;

use crate::grf::Grf;

const NPC_IDENTITY: &str = "data\\luafiles514\\lua files\\datainfo\\npcidentity.lub";
const JOB_NAME: &str = "data\\luafiles514\\lua files\\datainfo\\jobname.lub";
const PET_INFO: &str = "data\\luafiles514\\lua files\\datainfo\\petinfo.lub";
const TITLE_TABLE: &str = "data\\luafiles514\\lua files\\datainfo\\titletable.lub";
/// Not in data.grf: the translation's System folder has it.
const ACHIEVEMENTS: &str = "system\\achievements.lub";
/// The same table under the name older data (kRO 2022, its translations) gives it.
const ACHIEVEMENT_LIST: &str = "system\\achievement_list.lub";
const ITEM_INFO: &str = "data\\luafiles514\\lua files\\datainfo\\iteminfo.lub";
const TRANSLATED_ITEM_INFO: &str = "system\\luafiles514\\iteminfo.lua";
const ACCESSORY_ID: &str = "data\\luafiles514\\lua files\\datainfo\\accessoryid.lub";
const ACCESSORY_NAME: &str = "data\\luafiles514\\lua files\\datainfo\\accname.lub";
const WEAPON_TABLE: &str = "data\\luafiles514\\lua files\\datainfo\\weapontable.lub";
const ROBE_ID: &str = "data\\luafiles514\\lua files\\datainfo\\spriterobeid.lub";
const ROBE_NAME: &str = "data\\luafiles514\\lua files\\datainfo\\spriterobename.lub";
/// What decides where garments and headgear go among a character's layers: the garment tables (by job, action and
/// frame), the garments worn over the headgear, the headgear priorities, and the headgear Doram wear moved.
const LAYER_RULES: [&str; 14] = [
    JOB_INHERIT,
    ROBE_ID,
    ROBE_NAME,
    "data\\luafiles514\\lua files\\datainfo\\spriterobename_f.lub",
    "data\\luafiles514\\lua files\\spreditinfo\\2dlayerdir_f.lub",
    "data\\luafiles514\\lua files\\spreditinfo\\_new_biglayerdir_male.lub",
    "data\\luafiles514\\lua files\\spreditinfo\\_new_biglayerdir_female.lub",
    "data\\luafiles514\\lua files\\spreditinfo\\_new_smalllayerdir_male.lub",
    "data\\luafiles514\\lua files\\spreditinfo\\_new_smalllayerdir_female.lub",
    "data\\luafiles514\\lua files\\spreditinfo\\_new_2dlayerdir_f.lub",
    ACCESSORY_ID,
    "data\\luafiles514\\lua files\\datainfo\\tb_layer_priority.lub",
    "data\\luafiles514\\lua files\\offsetitempos\\offsetitempos_f.lub",
    "data\\luafiles514\\lua files\\offsetitempos\\offsetitempos.lub",
];
/// Defines JOBID, which the job name and skill tables use.
const JOB_INHERIT: &str = "data\\luafiles514\\lua files\\skillinfoz\\jobinheritlist.lub";
const PC_JOB_NAME: &str = "data\\luafiles514\\lua files\\datainfo\\pcjobname.lub";
const SKILL_ID: &str = "data\\luafiles514\\lua files\\skillinfoz\\skillid.lub";
const SKILL_INFO: &str = "data\\luafiles514\\lua files\\skillinfoz\\skillinfolist.lub";
/// Defines JobSkillTab, which the skill tree needs.
const SKILL_INFO_F: &str = "data\\luafiles514\\lua files\\skillinfoz\\skillinfo_f.lub";
const SKILL_TREE: &str = "data\\luafiles514\\lua files\\skillinfoz\\skilltreeview.lub";
const EFST_IDS: &str = "data\\luafiles514\\lua files\\stateicon\\efstids.lub";
const STATE_ICON_IMAGES: &str = "data\\luafiles514\\lua files\\stateicon\\stateiconimginfo.lub";
/// Laphine synthesis boxes (`tblLapineDdukddakBox`).
const SYNTHESIS_TABLE: &str = "data\\luafiles514\\lua files\\datainfo\\lapineddukddakbox.lub";
/// Laphine upgrade boxes (`tblLapineUpgradeBox`).
const UPGRADE_TABLE: &str = "data\\luafiles514\\lua files\\datainfo\\lapineupgradebox.lub";
/// The world maps: their names (the language file first, which the others use), list, colors and areas.
const WORLD_VIEW: [&str; 4] = [
    "data\\luafiles514\\lua files\\worldviewdata\\worldviewdata_language.lub",
    "data\\luafiles514\\lua files\\worldviewdata\\worldviewdata_list.lub",
    "data\\luafiles514\\lua files\\worldviewdata\\worldviewdata_info.lub",
    "data\\luafiles514\\lua files\\worldviewdata\\worldviewdata_table.lub",
];

/// Runs client Lua files in order; precompiled ones are converted from the client's 32-bit bytecode first.
fn run(grf: &Grf, files: &[&str]) -> io::Result<Lua> {
    let lua = new_lua()?;
    exec(&lua, grf, files)?;
    Ok(lua)
}

fn new_lua() -> io::Result<Lua> {
    // Data tables only need these; no file or OS access for files that may come from a translation pack.
    Lua::new_with(mlua::StdLib::TABLE | mlua::StdLib::STRING | mlua::StdLib::MATH, mlua::LuaOptions::default()).map_err(lua_error)
}

/// Runs more client Lua files in `lua`.
fn exec(lua: &Lua, grf: &Grf, files: &[&str]) -> io::Result<()> {
    for file in files {
        exec_bytes(lua, file, grf.read(file)?)?;
    }
    Ok(())
}

/// Runs one file's bytes, bytecode or plain source.
fn exec_bytes(lua: &Lua, file: &str, bytes: Vec<u8>) -> io::Result<()> {
    let bytes = match lunify::unify(&bytes, &lunify::Format::default(), &lunify::Settings::default()) {
        Ok(converted) => converted,
        // Not bytecode: plain Lua source.
        Err(lunify::LunifyError::IncorrectSignature) => bytes,
        Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{file}: {error:?}"))),
    };
    lua.load(&bytes).set_name(file).exec().map_err(|error| io::Error::other(format!("{file}: {error}")))
}

/// Dev aid: runs `files`, evaluates `expression` and lists the result (tables nested up to `depth`, `limit` entries
/// a table, keys sorted) with strings decoded.
pub fn dump(grf: &Grf, files: &[&str], expression: &str, depth: usize, limit: usize) -> io::Result<String> {
    fn show(value: &mlua::Value, depth: usize, limit: usize, indent: usize, out: &mut String) {
        match value {
            mlua::Value::String(text) => out.push_str(&format!("{:?}", decode(text))),
            mlua::Value::Table(table) if depth > 0 => {
                let mut pairs: Vec<(String, mlua::Value)> =
                    table.pairs::<mlua::Value, mlua::Value>().flatten().map(|(key, value)| (key_text(&key), value)).collect();
                pairs.sort_by(|a, b| (a.0.len(), &a.0).cmp(&(b.0.len(), &b.0)));
                out.push_str(&format!("{{ ({} entries)\n", pairs.len()));
                for (key, value) in pairs.iter().take(limit) {
                    out.push_str(&format!("{}{key} = ", "  ".repeat(indent + 1)));
                    show(value, depth - 1, limit, indent + 1, out);
                    out.push('\n');
                }
                out.push_str(&format!("{}}}", "  ".repeat(indent)));
            }
            other => out.push_str(&format!("{other:?}")),
        }
    }
    fn key_text(key: &mlua::Value) -> String {
        match key {
            mlua::Value::String(text) => decode(text),
            other => format!("{other:?}").trim_start_matches("Integer(").trim_end_matches(')').to_owned(),
        }
    }
    let lua = run(grf, files)?;
    let value: mlua::Value = lua.load(format!("return {expression}")).eval().map_err(lua_error)?;
    let mut out = String::new();
    show(&value, depth, limit, 0, &mut out);
    Ok(out)
}

fn lua_error(error: mlua::Error) -> io::Error {
    io::Error::other(error.to_string())
}

/// The client's Lua strings are EUC-KR; in the newer tables those bytes were read as Windows-1252 and saved as
/// UTF-8, which this undoes.
fn decode(text: &mlua::String) -> String {
    let bytes = text.as_bytes();
    if let Ok(utf8) = std::str::from_utf8(&bytes)
        && !utf8.is_ascii()
    {
        let (original, _, unmappable) = encoding_rs::WINDOWS_1252.encode(utf8);
        return if unmappable { utf8.to_owned() } else { encoding_rs::EUC_KR.decode_without_bom_handling(&original).0.into_owned() };
    }
    encoding_rs::EUC_KR.decode_without_bom_handling(&bytes).0.into_owned()
}

/// A Lua table of numbers to names.
fn names<K: TryFrom<i64> + std::hash::Hash + Eq>(table: &mlua::Table) -> io::Result<HashMap<K, String>> {
    let mut names = HashMap::new();
    for pair in table.pairs::<i64, mlua::String>() {
        let (id, name) = pair.map_err(lua_error)?;
        if let Ok(id) = K::try_from(id) {
            names.insert(id, decode(&name));
        }
    }
    Ok(names)
}

/// Sprite names of NPCs and monsters by job ID (`JobNameTable`), e.g. 45 → "1_ETC_01".
pub fn job_names(grf: &Grf) -> io::Result<HashMap<u16, String>> {
    let lua = run(grf, &[NPC_IDENTITY, JOB_NAME])?;
    let table: mlua::Table = lua.globals().get("JobNameTable").map_err(lua_error)?;
    names(&table)
}

/// The pet tables (petinfo.lub), by monster ID or (accessories) item ID.
#[derive(Debug, Default)]
pub struct PetTables {
    /// Its talk table's name, e.g. 1002 → "poring".
    pub names: HashMap<u16, String>,
    /// Its picture in `유저인터페이스\illust\`, e.g. "펫_포링.bmp".
    pub illustrations: HashMap<u16, String>,
    /// The ACT an accessory moves its pet by, e.g. 10013 → "poring_책가방.act".
    pub accessory_acts: HashMap<u16, String>,
}

pub fn pet_tables(grf: &Grf) -> io::Result<PetTables> {
    let lua = run(grf, &[NPC_IDENTITY])?;
    // A translated petinfo may name monsters this npcidentity doesn't know: they all go under one unused key.
    lua.load("setmetatable(jobtbl, { __index = function() return -1 end })").exec().map_err(lua_error)?;
    exec(&lua, grf, &[PET_INFO])?;
    let table = |name: &str| names(&lua.globals().get::<mlua::Table>(name).map_err(lua_error)?);
    Ok(PetTables { names: table("PetNameTable")?, illustrations: table("PetIllustNameTable")?, accessory_acts: table("PetAccActNameTable")? })
}

/// An achievement as the client's table (achievements.lub) describes it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct AchievementInfo {
    /// The objectives have targets the server's counts go toward; otherwise they're a checklist.
    pub counted: bool,
    /// Its icon's name: `achievement_re\icon_<group>.bmp`, lowercase.
    pub group: String,
    /// Its tab and sub-tab.
    pub major: u8,
    pub minor: u8,
    pub title: String,
    pub summary: String,
    pub details: String,
    /// Each objective's text and target (0 for a checklist's).
    pub objectives: Vec<(String, u32)>,
    /// The reward: an item, a title and a buff (0 for none of each).
    pub reward_item: u32,
    pub reward_title: u32,
    pub reward_buff: u32,
    pub score: u32,
}

/// The achievements by ID, from the System folder's achievements.lub (or achievement_list.lub).
pub fn achievements(grf: &Grf) -> io::Result<HashMap<u32, AchievementInfo>> {
    achievement_table(achievement_source(|path| grf.read(path))?)
}

/// The achievement table's source: achievements.lub, else achievement_list.lub.
fn achievement_source(read: impl Fn(&str) -> io::Result<Vec<u8>>) -> io::Result<Vec<u8>> {
    read(ACHIEVEMENTS).or_else(|_| read(ACHIEVEMENT_LIST))
}

/// Reads `achievement_tbl` from the file's source; its `main()` (which calls the official client's functions) isn't run.
pub fn achievement_table(source: Vec<u8>) -> io::Result<HashMap<u32, AchievementInfo>> {
    let lua = new_lua()?;
    exec_bytes(&lua, ACHIEVEMENTS, source)?;
    let table: mlua::Table = lua.globals().get("achievement_tbl").map_err(lua_error)?;
    let text = |table: &mlua::Table, key: &str| table.get::<Option<mlua::String>>(key).ok().flatten().map(|text| decode(&text)).unwrap_or_default();
    let number = |table: &mlua::Table, key: &str| table.get::<Option<u32>>(key).ok().flatten().unwrap_or(0);
    let mut achievements = HashMap::new();
    for pair in table.pairs::<u32, mlua::Table>() {
        let (id, entry) = pair.map_err(lua_error)?;
        let content: Option<mlua::Table> = entry.get("content").ok().flatten();
        let reward: Option<mlua::Table> = entry.get("reward").ok().flatten();
        let mut objectives = Vec::new();
        if let Ok(Some(resource)) = entry.get::<Option<mlua::Table>>("resource") {
            for index in 1..=resource.raw_len() {
                if let Ok(objective) = resource.get::<mlua::Table>(index) {
                    objectives.push((text(&objective, "text"), number(&objective, "count")));
                }
            }
        }
        let info = AchievementInfo {
            counted: number(&entry, "UI_Type") == 1,
            group: text(&entry, "group").to_lowercase(),
            major: number(&entry, "major") as u8,
            minor: number(&entry, "minor") as u8,
            title: text(&entry, "title"),
            summary: content.as_ref().map(|content| text(content, "summary")).unwrap_or_default(),
            details: content.as_ref().map(|content| text(content, "details")).unwrap_or_default(),
            objectives,
            reward_item: reward.as_ref().map_or(0, |reward| number(reward, "item")),
            reward_title: reward.as_ref().map_or(0, |reward| number(reward, "title")),
            reward_buff: reward.as_ref().map_or(0, |reward| number(reward, "buff")),
            score: number(&entry, "score"),
        };
        achievements.insert(id, info);
    }
    Ok(achievements)
}

/// The titles achievements give (`title_tbl`), e.g. 1000 → "Junction of life".
pub fn titles(grf: &Grf) -> io::Result<HashMap<u32, String>> {
    names(&run(grf, &[TITLE_TABLE])?.globals().get::<mlua::Table>("title_tbl").map_err(lua_error)?)
}

/// Job names the client shows players (`PCJobNameTable`), e.g. 0 → "Novice".
pub fn pc_job_names(grf: &Grf) -> io::Result<HashMap<u16, String>> {
    let lua = run(grf, &[JOB_INHERIT, PC_JOB_NAME])?;
    names(&lua.globals().get::<mlua::Table>("PCJobNameTable").map_err(lua_error)?)
}

/// Sprite names for what characters wear, by view ID.
#[derive(Debug, Default)]
pub struct LookTables {
    /// Headgear sprite name endings (data\sprite\악세사리\<sex>\<sex><name>).
    pub headgears: HashMap<u16, String>,
    /// Weapon sprite name endings (data\sprite\인간족\<class>\<class>_<sex><name>).
    pub weapons: HashMap<u32, String>,
    /// Weapon view IDs beyond the basic types, and the type each one is.
    pub weapon_types: HashMap<u32, u32>,
    /// Garment folders (data\sprite\로브\<name>\<sex>\<class>_<sex>).
    pub robes: HashMap<u16, String>,
    /// Item IDs to the view they show as (iteminfo's ClassNum): weapons and shields arrive as item IDs.
    pub item_views: HashMap<u32, u32>,
    /// Where garments and headgear go among a character's layers; None without the tables.
    pub layers: Option<LayerRules>,
}

/// The data's own rules for layering what a character wears, run in a Lua VM of their own.
pub struct LayerRules(Lua);

impl std::fmt::Debug for LayerRules {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("LayerRules")
    }
}

impl LayerRules {
    pub fn load(grf: &Grf) -> io::Result<Self> {
        run(grf, &LAYER_RULES).map(Self)
    }

    fn call<R: mlua::FromLuaMulti>(&self, function: &str, arguments: impl mlua::IntoLuaMulti) -> Option<R> {
        self.0.globals().get::<mlua::Function>(function).ok()?.call(arguments).ok()
    }

    /// Whether garment `robe` goes over the body (else behind everything) on a character of `job` and `sex` (0
    /// female) in action `action` (direction included, 0 to 103) at body frame `frame`.
    pub fn robe_over_body(&self, robe: u16, sex: u8, job: u16, action: usize, frame: usize) -> Option<bool> {
        self.call::<(bool, mlua::Value)>("_New_DrawOnTop", (robe, sex, job, action, frame)).map(|(over, _)| over)
    }

    /// Garments that, when over the body, go over the headgear too (bags).
    pub fn robe_over_headgear(&self, robe: u16) -> bool {
        self.call("IsTopLayer", robe).unwrap_or(false)
    }

    /// Where headgear `view` in `slot` (2 lower, 3 upper, 4 middle) goes facing `direction`: layers are drawn in order
    /// of these numbers, the body's numbered from 1; -1 hides it.
    pub fn headgear_priority(&self, direction: usize, slot: u8, view: u16) -> Option<i32> {
        self.call("GetLayerPriority", (direction, slot, view))
    }

    /// How far a Doram of `sex` has headgear `view` moved facing `direction` (sprite pixels), if at all.
    pub fn doram_headgear_offset(&self, view: u16, direction: usize, sex: u8) -> Option<[i32; 2]> {
        let (x, y) = self.call::<(Option<i32>, Option<i32>)>("OffsetItemPos_GetOffsetForDoram", (view, direction, sex))?;
        Some([x?, y?])
    }
}

/// Looks below this are view IDs; from here on they're item IDs.
const FIRST_ITEM_ID: u32 = 500;

impl LookTables {
    /// The view ID of a weapon or shield look.
    pub fn view(&self, look: u32) -> u32 {
        if look < FIRST_ITEM_ID { look } else { self.item_views.get(&look).copied().filter(|&view| view > 0).unwrap_or(look) }
    }

    /// A weapon look's basic type (dagger, sword, bow, ...), which decides the attack motion.
    pub fn weapon_type(&self, look: u32) -> u32 {
        let view = self.view(look);
        self.weapon_types.get(&view).copied().unwrap_or(view)
    }
}

pub fn look_tables(grf: &Grf) -> io::Result<LookTables> {
    let table = |lua: &Lua, name: &str| -> io::Result<mlua::Table> { lua.globals().get(name).map_err(lua_error) };
    let accessories = run(grf, &[ACCESSORY_ID, ACCESSORY_NAME])?;
    let weapons = run(grf, &[WEAPON_TABLE])?;
    let robes = run(grf, &[ROBE_ID, ROBE_NAME])?;
    let mut weapon_types = HashMap::new();
    for pair in table(&weapons, "Expansion_Weapon_IDs")?.pairs::<u32, u32>() {
        let (view, kind) = pair.map_err(lua_error)?;
        weapon_types.insert(view, kind);
    }
    Ok(LookTables {
        headgears: names(&table(&accessories, "AccNameTable")?)?,
        weapons: names(&table(&weapons, "WeaponNameTable")?)?,
        weapon_types,
        robes: names(&table(&robes, "RobeNameTable")?)?,
        item_views: HashMap::new(),
        layers: LayerRules::load(grf).ok(),
    })
}

/// What the client shows for an item, identified and not.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemInfo {
    pub name: String,
    pub unidentified_name: String,
    /// File name of the ground sprite (data\sprite\아이템\) and the icon (data\texture\유저인터페이스\item\).
    pub resource: String,
    pub unidentified_resource: String,
    /// Card slots.
    pub slots: u8,
    /// The view the item shows as when worn (weapon type, shield view, ...).
    pub class_num: u32,
    /// What the item info window says about it (lines with ^RRGGBB colors), known and unknown.
    pub description: Vec<String>,
    pub unidentified_description: Vec<String>,
}

impl ItemInfo {
    pub fn name(&self, identified: bool) -> &str {
        if identified { &self.name } else { &self.unidentified_name }
    }

    pub fn resource(&self, identified: bool) -> &str {
        if identified { &self.resource } else { &self.unidentified_resource }
    }

    pub fn description(&self, identified: bool) -> &[String] {
        if identified { &self.description } else { &self.unidentified_description }
    }
}

/// Every item the client knows, by item ID (`tbl` in iteminfo).
pub fn item_info(grf: &Grf) -> io::Result<HashMap<u32, ItemInfo>> {
    // A translation's item table (System\LuaFiles514\itemInfo.lua) goes before the client's own.
    let lua = run(grf, &[TRANSLATED_ITEM_INFO]).or_else(|_| run(grf, &[ITEM_INFO]))?;
    let table: mlua::Table = lua.globals().get("tbl").map_err(lua_error)?;
    let text = |entry: &mlua::Table, key: &str| entry.get::<mlua::String>(key).map(|text| decode(&text)).unwrap_or_default();
    let lines = |entry: &mlua::Table, key: &str| -> Vec<String> {
        entry.get::<mlua::Table>(key).map(|lines| lines.sequence_values::<mlua::String>().flatten().map(|line| decode(&line)).collect()).unwrap_or_default()
    };
    let mut items = HashMap::new();
    for pair in table.pairs::<i64, mlua::Table>() {
        let (id, entry) = pair.map_err(lua_error)?;
        if let Ok(id) = u32::try_from(id) {
            items.insert(
                id,
                ItemInfo {
                    name: text(&entry, "identifiedDisplayName"),
                    unidentified_name: text(&entry, "unidentifiedDisplayName"),
                    resource: text(&entry, "identifiedResourceName"),
                    unidentified_resource: text(&entry, "unidentifiedResourceName"),
                    slots: entry.get::<u8>("slotCount").unwrap_or(0),
                    class_num: entry.get::<u32>("ClassNum").unwrap_or(0),
                    description: lines(&entry, "identifiedDescriptionName"),
                    unidentified_description: lines(&entry, "unidentifiedDescriptionName"),
                },
            );
        }
    }
    Ok(items)
}

/// A skill as the client describes it (`SKILL_INFO_LIST`).
#[derive(Debug, Clone, Default)]
pub struct SkillInfo {
    /// The skill's code name, which also names its icon (AL_HEAL → item\al_heal.bmp).
    pub resource: String,
    pub name: String,
    pub max_level: u16,
}

pub fn skill_info(grf: &Grf) -> io::Result<HashMap<u16, SkillInfo>> {
    let lua = run(grf, &[JOB_INHERIT, SKILL_ID, SKILL_INFO])?;
    let list: mlua::Table = lua.globals().get("SKILL_INFO_LIST").map_err(lua_error)?;
    let mut skills = HashMap::new();
    for pair in list.pairs::<i64, mlua::Table>() {
        let Ok((id, entry)) = pair else { continue };
        let text = |value: mlua::Result<mlua::String>| value.map(|text| decode(&text)).unwrap_or_default();
        let info = SkillInfo { resource: text(entry.get(1)), name: text(entry.get("SkillName")), max_level: entry.get("MaxLv").unwrap_or(0) };
        if let Ok(id) = u16::try_from(id) {
            skills.insert(id, info);
        }
    }
    Ok(skills)
}

/// Where each job's skills sit in the skill window (`SKILL_TREEVIEW_FOR_JOB`: places in a 7-wide grid, from 0) and
/// which job each job comes from (`JOB_INHERIT_LIST`).
#[derive(Debug, Default)]
pub struct SkillTrees {
    pub trees: HashMap<u16, Vec<(usize, u16)>>,
    pub parents: HashMap<u16, u16>,
}

impl SkillTrees {
    /// A job and the jobs it came from, novice first: the skill window's tabs.
    pub fn lineage(&self, job: u16) -> Vec<u16> {
        let mut jobs = vec![job];
        while let Some(&parent) = self.parents.get(jobs.last().unwrap()) {
            if jobs.contains(&parent) {
                break;
            }
            jobs.push(parent);
        }
        if jobs.last() != Some(&0) {
            jobs.push(0);
        }
        jobs.reverse();
        jobs
    }
}

pub fn skill_trees(grf: &Grf) -> io::Result<SkillTrees> {
    let lua = run(grf, &[JOB_INHERIT, SKILL_ID, SKILL_INFO_F, SKILL_TREE])?;
    let globals = lua.globals();
    let mut trees = SkillTrees::default();
    let views: mlua::Table = globals.get("SKILL_TREEVIEW_FOR_JOB").map_err(lua_error)?;
    for pair in views.pairs::<i64, mlua::Table>() {
        let Ok((job, places)) = pair else { continue };
        let mut tree: Vec<(usize, u16)> =
            places.pairs::<i64, i64>().flatten().filter_map(|(place, skill)| Some((usize::try_from(place).ok()?, u16::try_from(skill).ok()?))).collect();
        tree.sort_unstable();
        trees.trees.insert(job as u16, tree);
    }
    for list in ["JOB_INHERIT_LIST", "JOB_INHERIT_LIST2"] {
        if let Ok(table) = globals.get::<mlua::Table>(list) {
            trees.parents.extend(table.pairs::<i64, i64>().flatten().filter_map(|(job, parent)| Some((u16::try_from(job).ok()?, u16::try_from(parent).ok()?))));
        }
    }
    Ok(trees)
}

/// Status effect icons by EFST id (`StateIconImgList`): the file under data\texture\effect\ and its priority.
pub fn status_icons(grf: &Grf) -> io::Result<HashMap<u16, (u8, String)>> {
    let lua = run(grf, &[EFST_IDS, STATE_ICON_IMAGES])?;
    let list: mlua::Table = lua.globals().get("StateIconImgList").map_err(lua_error)?;
    // The effects older than the Lua table, which the client knows by itself.
    let mut icons: HashMap<u16, (u8, String)> = include_str!("status_icons.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (id, file) = line.split_once('\t')?;
            Some((id.parse().ok()?, (0, file.to_owned())))
        })
        .collect();
    for pair in list.pairs::<i64, mlua::Table>() {
        let Ok((priority, files)) = pair else { continue };
        for (efst, file) in names::<u16>(&files)? {
            icons.insert(efst, (priority as u8, file));
        }
    }
    Ok(icons)
}

/// A field's box on a world map's picture (in its pixels: left, top, right, bottom), by its map's name (lowercase,
/// no extension).
#[derive(Debug, Clone, PartialEq)]
pub struct WorldArea {
    pub map: String,
    pub rect: [f32; 4],
}

/// A world map: its name, its picture (under the UI texture folder) and its fields.
#[derive(Debug, Clone, PartialEq)]
pub struct World {
    pub name: String,
    pub image: String,
    pub fields: Vec<WorldArea>,
}

/// The world maps (`World_List`, each naming its table of fields: region, map, box, monster and level), and the
/// box colors as red, green, blue and alpha (`worldview_info`): where the player is, and what's pointed at.
pub fn worlds(grf: &Grf) -> io::Result<(Vec<World>, [[u8; 4]; 2])> {
    let lua = run(grf, &WORLD_VIEW)?;
    let globals = lua.globals();
    let list: mlua::Table = globals.get("World_List").map_err(lua_error)?;
    let mut worlds = Vec::new();
    for entry in list.sequence_values::<mlua::Table>() {
        let entry = entry.map_err(lua_error)?;
        let text = |index: usize| entry.get::<mlua::String>(index).map(|text| decode(&text)).unwrap_or_default();
        let Ok(table) = globals.get::<mlua::Table>(text(2)) else { continue };
        let fields = table
            .sequence_values::<mlua::Table>()
            .flatten()
            .filter_map(|field| {
                let map = decode(&field.get::<mlua::String>(2).ok()?).to_lowercase();
                let rect = [3, 4, 5, 6].map(|index| field.get::<f32>(index).unwrap_or(0.0));
                Some(WorldArea { map: map.trim_end_matches(".rsw").to_owned(), rect })
            })
            .collect();
        worlds.push(World { name: text(1), image: text(4), fields });
    }
    let info: mlua::Table = globals.get("worldview_info").map_err(lua_error)?;
    let color = |name: &str| ["ColorR", "ColorG", "ColorB", "Alpha"].map(|part| info.get::<u8>(format!("{name}_{part}")).unwrap_or(0));
    Ok((worlds, [color("Run"), color("SelectBox")]))
}

/// One material a synthesis box accepts.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthesisSource {
    pub name: String,
    pub amount: u16,
    pub id: u32,
}

/// A synthesis box, keyed later by `ItemID`. `NeedSource_String` is left out.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthesisBox {
    pub need_count: u32,
    pub refine_min: u32,
    pub refine_max: u32,
    pub sources: Vec<SynthesisSource>,
}

/// One item an upgrade box accepts.
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeTarget {
    pub name: String,
    pub id: u32,
}

/// An upgrade box, keyed later by `ItemID`.
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeBox {
    pub refine_min: u32,
    pub refine_max: u32,
    /// Random options the item must have. The client doesn't track those, so the window doesn't check it.
    pub option_min: u32,
    /// `NotSocketEnchantItem`: no cards allowed.
    pub no_cards: bool,
    pub targets: Vec<UpgradeTarget>,
}

fn lua_u32(table: &mlua::Table, key: &str) -> u32 {
    table.get::<Option<u32>>(key).ok().flatten().unwrap_or(0)
}

fn lua_flag(table: &mlua::Table, key: &str) -> bool {
    match table.get::<mlua::Value>(key).ok() {
        Some(mlua::Value::Boolean(value)) => value,
        Some(mlua::Value::Integer(value)) => value != 0,
        Some(mlua::Value::Number(value)) => value != 0.0,
        _ => false,
    }
}

fn lua_name(table: &mlua::Table, index: i64) -> String {
    table.get::<mlua::String>(index).map(|text| decode(&text)).unwrap_or_default()
}

/// `tblLapineDdukddakBox.sources` from the file's bytes. Its functions aren't called.
pub fn synthesis_table(source: Vec<u8>) -> io::Result<HashMap<u32, SynthesisBox>> {
    let lua = new_lua()?;
    exec_bytes(&lua, SYNTHESIS_TABLE, source)?;
    let root: mlua::Table = lua.globals().get("tblLapineDdukddakBox").map_err(lua_error)?;
    let sources: mlua::Table = root.get("sources").map_err(lua_error)?;
    let mut boxes = HashMap::new();
    for pair in sources.pairs::<mlua::Value, mlua::Table>() {
        let (_, entry) = pair.map_err(lua_error)?;
        let item_id = lua_u32(&entry, "ItemID");
        if item_id == 0 {
            continue;
        }
        let mut items = Vec::new();
        if let Ok(list) = entry.get::<mlua::Table>("SourceItems") {
            for row in list.sequence_values::<mlua::Table>() {
                let row = row.map_err(lua_error)?;
                let id = row.get::<u32>(3).unwrap_or(0);
                if id == 0 {
                    continue;
                }
                items.push(SynthesisSource { name: lua_name(&row, 1), amount: row.get::<u32>(2).unwrap_or(1) as u16, id });
            }
        }
        let refine_max = entry.get::<Option<u32>>("NeedRefineMax").ok().flatten().unwrap_or(u32::from(u8::MAX));
        boxes.insert(
            item_id,
            SynthesisBox { need_count: lua_u32(&entry, "NeedCount"), refine_min: lua_u32(&entry, "NeedRefineMin"), refine_max, sources: items },
        );
    }
    Ok(boxes)
}

/// `tblLapineUpgradeBox.targets` from the file's bytes.
pub fn upgrade_table(source: Vec<u8>) -> io::Result<HashMap<u32, UpgradeBox>> {
    let lua = new_lua()?;
    exec_bytes(&lua, UPGRADE_TABLE, source)?;
    let root: mlua::Table = lua.globals().get("tblLapineUpgradeBox").map_err(lua_error)?;
    let targets: mlua::Table = root.get("targets").map_err(lua_error)?;
    let mut boxes = HashMap::new();
    for pair in targets.pairs::<mlua::Value, mlua::Table>() {
        let (_, entry) = pair.map_err(lua_error)?;
        let item_id = lua_u32(&entry, "ItemID");
        if item_id == 0 {
            continue;
        }
        let mut items = Vec::new();
        if let Ok(list) = entry.get::<mlua::Table>("TargetItems") {
            for row in list.sequence_values::<mlua::Table>() {
                let row = row.map_err(lua_error)?;
                let id = row.get::<u32>(2).unwrap_or(0);
                if id == 0 {
                    continue;
                }
                items.push(UpgradeTarget { name: lua_name(&row, 1), id });
            }
        }
        let refine_max = entry.get::<Option<u32>>("NeedRefineMax").ok().flatten().unwrap_or(u32::from(u8::MAX));
        boxes.insert(
            item_id,
            UpgradeBox {
                refine_min: lua_u32(&entry, "NeedRefineMin"),
                refine_max,
                option_min: lua_u32(&entry, "NeedOptionNumMin"),
                no_cards: lua_flag(&entry, "NotSocketEnchantItem"),
                targets: items,
            },
        );
    }
    Ok(boxes)
}

/// Synthesis boxes from data.grf, by the box's item ID.
pub fn synthesis_boxes(grf: &Grf) -> io::Result<HashMap<u32, SynthesisBox>> {
    synthesis_table(grf.read(SYNTHESIS_TABLE)?)
}

/// Upgrade boxes from data.grf, by the box's item ID.
pub fn upgrade_boxes(grf: &Grf) -> io::Result<HashMap<u32, UpgradeBox>> {
    upgrade_table(grf.read(UPGRADE_TABLE)?)
}

#[cfg(test)]
mod tests {
    use crate::grf::Grf;

    #[test]
    fn finds_the_achievement_table_under_either_name() {
        let only = |name: &'static str| {
            move |path: &str| -> std::io::Result<Vec<u8>> {
                if path == name {
                    Ok(b"achievement_tbl = { [240000] = { title = \"First steps\" } }".to_vec())
                } else {
                    Err(std::io::ErrorKind::NotFound.into())
                }
            }
        };
        for name in [super::ACHIEVEMENTS, super::ACHIEVEMENT_LIST] {
            let table = super::achievement_table(super::achievement_source(only(name)).unwrap()).unwrap();
            assert_eq!(table[&240000].title, "First steps", "{name}");
        }
        assert!(super::achievement_source(only("elsewhere")).is_err());
    }

    #[test]
    fn reads_achievements() {
        let source = r#"
            achievement_tbl = {
                [200000] = {
                    UI_Type = 0, group = "GOAL_LEVEL", major = 1, minor = 0,
                    title = "Acquire the first aura!",
                    content = { summary = "Achieve Base Level 99!", details = "Achieve Base Level 99." },
                    resource = { [1] = { text = "Base Level 99" } },
                    reward = { title = 1000, buff = 10, item = 12549 },
                    score = 50
                },
                [120001] = {
                    UI_Type = 1, group = "ADVENTURE", major = 2, minor = 3, title = "Hunter",
                    resource = { [1] = { text = "Porings", count = 10 }, [2] = { text = "Lunatics", count = 5 } },
                    reward = {}, score = 10
                }
            }
            function main() return CallTheClient() end
        "#;
        let table = super::achievement_table(source.as_bytes().to_vec()).unwrap();
        let aura = &table[&200000];
        assert_eq!((aura.counted, aura.group.as_str(), aura.major, aura.minor), (false, "goal_level", 1, 0));
        assert_eq!((aura.title.as_str(), aura.summary.as_str()), ("Acquire the first aura!", "Achieve Base Level 99!"));
        assert_eq!((aura.reward_item, aura.reward_title, aura.reward_buff, aura.score), (12549, 1000, 10, 50));
        assert_eq!(aura.objectives, [("Base Level 99".to_owned(), 0)]);
        let hunter = &table[&120001];
        assert!(hunter.counted);
        assert_eq!(hunter.objectives, [("Porings".to_owned(), 10), ("Lunatics".to_owned(), 5)]);
        assert_eq!((hunter.reward_title, hunter.details.as_str()), (0, ""));
    }

    #[test]
    fn reads_laphine_boxes() {
        let synthesis = r#"
            tblLapineDdukddakBox = {
                sources = {
                    IDTest = {
                        ItemID = 100000, NeedCount = 3, NeedRefineMin = 0, NeedRefineMax = 6,
                        NeedSource_String = "leave this out",
                        SourceItems = { { "Jellopy", 1, 909 }, { "Fluff", 1, 914 }, { "Shell", 1, 935 } }
                    }
                }
            }
            function set_resource_item_list() end
        "#;
        let boxes = super::synthesis_table(synthesis.as_bytes().to_vec()).unwrap();
        let test = &boxes[&100000];
        assert_eq!((test.need_count, test.refine_min, test.refine_max), (3, 0, 6));
        assert_eq!(
            test.sources,
            [
                super::SynthesisSource { name: "Jellopy".into(), amount: 1, id: 909 },
                super::SynthesisSource { name: "Fluff".into(), amount: 1, id: 914 },
                super::SynthesisSource { name: "Shell".into(), amount: 1, id: 935 },
            ]
        );

        let upgrade = r#"
            tblLapineUpgradeBox = {
                targets = {
                    GrayWolf = {
                        ItemID = 100626, NeedRefineMin = 0, NeedRefineMax = 6,
                        NeedOptionNumMin = 2, NotSocketEnchantItem = true,
                        NeedSource_String = "leave this out",
                        TargetItems = { { "Gray Wolf Suit", 450177 } }
                    },
                    Open = { ItemID = 100001, NeedRefineMin = 1, TargetItems = { { "Knife", 1201 } } }
                }
            }
        "#;
        let boxes = super::upgrade_table(upgrade.as_bytes().to_vec()).unwrap();
        let wolf = &boxes[&100626];
        assert_eq!((wolf.refine_min, wolf.refine_max, wolf.option_min, wolf.no_cards), (0, 6, 2, true));
        assert_eq!(wolf.targets, [super::UpgradeTarget { name: "Gray Wolf Suit".into(), id: 450177 }]);
        let open = &boxes[&100001];
        assert_eq!((open.refine_max, open.no_cards, open.targets[0].id), (u32::from(u8::MAX), false, 1201));
    }

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn reads_the_world_maps() {
        let Some(grf) = Grf::for_tests() else { return };
        let (worlds, colors) = super::worlds(&grf).unwrap();
        let midgard = &worlds[0];
        assert_eq!(midgard.image, "worldmap.jpg");
        let prontera = midgard.fields.iter().find(|field| field.map == "prontera").unwrap();
        assert_eq!(prontera.rect, [812.0, 587.0, 870.0, 643.0]);
        assert_eq!(colors, [[85, 85, 255, 102], [149, 15, 15, 70]]);
    }

    /// Needs the official data.grf, at `RUSTNAROK_CLIENT_GRF` or `./data.grf`.
    #[test]
    fn reads_item_names_and_resources() {
        let Some(grf) = Grf::for_tests() else { return };
        let started = std::time::Instant::now();
        let items = super::item_info(&grf).unwrap();
        eprintln!("{} items in {:?}", items.len(), started.elapsed());
        assert!(items.len() > 5_000, "{} items", items.len());
        let jellopy = &items[&909];
        assert_eq!((jellopy.name.as_str(), jellopy.resource.as_str()), ("젤로피", "젤로피"));
        assert_eq!(items[&501].name, "빨간포션");
        assert_eq!(items[&1201].name, "나이프");
    }

    #[test]
    fn reads_skill_tables() {
        let Some(grf) = Grf::for_tests() else { return };
        let skills = super::skill_info(&grf).unwrap();
        let heal = &skills[&28];
        assert_eq!((heal.resource.as_str(), heal.name.as_str(), heal.max_level), ("AL_HEAL", "힐", 10));
        let trees = super::skill_trees(&grf).unwrap();
        // Novice: Basic Skill, First Aid and Play Dead down the first column.
        assert_eq!(trees.trees[&0], [(0, 1), (7, 142), (14, 143)]);
        assert!(trees.trees[&4].contains(&(2, 28)));
        // Priest (8) comes from Acolyte (4).
        assert_eq!(trees.lineage(8), [0, 4, 8]);
        assert_eq!(trees.lineage(0), [0]);
        let icons = super::status_icons(&grf).unwrap();
        assert_eq!(icons[&75].1, "DONTFORGETME.tga");
        assert_eq!(icons[&12].1, "민첩성증가.tga");
    }

    #[test]
    fn reads_job_names() {
        let Some(grf) = Grf::for_tests() else { return };
        let names = super::pc_job_names(&grf).unwrap();
        assert_eq!(names.get(&0).map(String::as_str), Some("Novice"));
        assert_eq!(names.get(&4008).map(String::as_str), Some("Lord Knight"));
    }

    #[test]
    fn reads_look_tables() {
        let Some(grf) = Grf::for_tests() else { return };
        let tables = super::look_tables(&grf).unwrap();
        assert_eq!(tables.headgears.get(&1).map(String::as_str), Some("_고글"));
        assert_eq!(tables.weapons.get(&1).map(String::as_str), Some("_단검"));
        assert!(tables.robes.len() > 100 && tables.weapon_types.len() > 10);
        assert_eq!(tables.weapon_type(1), 1);
    }

    #[test]
    fn reads_npc_and_monster_sprite_names() {
        let Some(grf) = Grf::for_tests() else { return };
        let names = super::job_names(&grf).unwrap();
        assert!(names.len() > 1000, "{} names", names.len());
        // Warp portals and Poring are the classic examples.
        assert_eq!(names.get(&45).map(String::as_str), Some("1_ETC_01"));
        assert_eq!(names.get(&1002).map(|name| name.to_lowercase()), Some("poring".into()));
    }
}
