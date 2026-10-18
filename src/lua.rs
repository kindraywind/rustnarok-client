//! The client's Lua data tables (compiled Lua 5.1 in data.grf), run in an embedded Lua 5.1.

use std::collections::{BTreeMap, HashMap};
use std::io;

use mlua::Lua;

use crate::grf::Grf;

const NPC_IDENTITY: &str = "data\\luafiles514\\lua files\\datainfo\\npcidentity.lub";
const JOB_NAME: &str = "data\\luafiles514\\lua files\\datainfo\\jobname.lub";
const PET_INFO: &str = "data\\luafiles514\\lua files\\datainfo\\petinfo.lub";
const DRESS_LIST: &str = "data\\luafiles514\\lua files\\dressroom\\jobdresslist.lub";
const DRESS_F: &str = "data\\luafiles514\\lua files\\dressroom\\dress_f.lub";
const TITLE_TABLE: &str = "data\\luafiles514\\lua files\\datainfo\\titletable.lub";
/// Not in data.grf: the translation's System folder has it.
const ACHIEVEMENTS: &str = "system\\achievements.lub";
/// The styling shop's functions, then its lists. They fill `StylingShop`.
const STYLING_SHOP_F: &str = "data\\luafiles514\\lua files\\stylingshop\\stylingshopinfo_f.lub";
const STYLING_SHOP: &str = "data\\luafiles514\\lua files\\stylingshop\\stylingshopinfo.lub";
/// The same table under the name older data (kRO 2022, its translations) gives it.
const ACHIEVEMENT_LIST: &str = "system\\achievement_list.lub";
const ITEM_INFO: &str = "data\\luafiles514\\lua files\\datainfo\\iteminfo.lub";
/// A translation's item table: under LuaFiles514, or (older translations) straight in System.
const TRANSLATED_ITEM_INFO: [&str; 2] = ["system\\luafiles514\\iteminfo.lua", "system\\iteminfo.lua"];
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
/// Each skill's description, line by line (`SKILL_DESCRIPT`).
const SKILL_DESCRIPT: &str = "data\\luafiles514\\lua files\\skillinfoz\\skilldescript.lub";
/// Defines JobSkillTab, which the skill tree needs.
const SKILL_INFO_F: &str = "data\\luafiles514\\lua files\\skillinfoz\\skillinfo_f.lub";
const SKILL_TREE: &str = "data\\luafiles514\\lua files\\skillinfoz\\skilltreeview.lub";
const EFST_IDS: &str = "data\\luafiles514\\lua files\\stateicon\\efstids.lub";
const STATE_ICON_IMAGES: &str = "data\\luafiles514\\lua files\\stateicon\\stateiconimginfo.lub";
/// What each status icon says when pointed at (`StateIconList`).
const STATE_ICON_INFO: &str = "data\\luafiles514\\lua files\\stateicon\\stateiconinfo.lub";
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

/// The battlegrounds the queue window lists (`EntryQueueList`); a translation's copy in the data folder has rathena's
/// names.
const ENTRY_QUEUE: &str = "data\\luafiles514\\lua files\\entryqueue\\entryqueuelist.lub";
/// Per monster job, how big its sprite is drawn (`tbl[job].MonsterSize`). The new file is read second and wins.
const JOB_IDENTITY: &str = "data\\luafiles514\\lua files\\datainfo\\jobidentity.lub";
const SHADOW_TABLE: &str = "data\\luafiles514\\lua files\\datainfo\\shadowtable.lub";
const NPC_LOCATION_RADIUS: &str = "data\\luafiles514\\lua files\\datainfo\\npclocationradius.lub";
const MONSTER_SIZE: &str = "system\\monster_size_effect.lub";
const MONSTER_SIZE_NEW: &str = "system\\monster_size_effect_new.lub";

/// A battleground the queue window lists.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Battleground {
    /// Sent to the server; rathena's `Name` in battleground_db.
    pub name: String,
    pub display: String,
    /// Team sizes: ours and theirs.
    pub sizes: [u16; 2],
    /// Applying alone, as a party, as a guild.
    pub applies: [bool; 3],
    /// Who may enter: 1 up to a level, 2 from a level, 3 a range; the levels.
    pub condition: (u8, [u32; 2]),
    /// What a win, a draw and a loss bring.
    pub rewards: [String; 3],
    pub goal: String,
    /// Its picture (battle_field\bat_a01.bmp).
    pub picture: String,
}

/// The client's navigation tables (navigation\navi_*_krpri.lub).
const NAVI_MAP: &str = "data\\luafiles514\\lua files\\navigation\\navi_map_krpri.lub";
const NAVI_LINK: &str = "data\\luafiles514\\lua files\\navigation\\navi_link_krpri.lub";
const NAVI_NPC: &str = "data\\luafiles514\\lua files\\navigation\\navi_npc_krpri.lub";
const NAVI_MOB: &str = "data\\luafiles514\\lua files\\navigation\\navi_mob_krpri.lub";

/// The navigation tables: maps with their shown names, the ways between maps, NPCs and monsters by map.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct NaviTables {
    pub maps: HashMap<String, String>,
    pub links: Vec<NaviLink>,
    pub npcs: Vec<NaviNpc>,
    pub monsters: Vec<NaviMonster>,
}

/// A way from one map to another: a warp (kind 200), an NPC that sends you (201), or a service (204, 205).
#[derive(Debug, Clone, PartialEq)]
pub struct NaviLink {
    pub map: String,
    pub kind: u32,
    pub name: String,
    pub cell: [u16; 2],
    pub to: String,
    pub to_cell: [u16; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct NaviNpc {
    pub map: String,
    pub name: String,
    pub cell: [u16; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct NaviMonster {
    pub map: String,
    pub id: u16,
    pub name: String,
}

pub fn navigation(grf: &Grf) -> io::Result<NaviTables> {
    navi_tables(&run(grf, &[NAVI_MAP, NAVI_LINK, NAVI_NPC, NAVI_MOB])?)
}

/// `Navi_Map` { name, shown name, .. }, `Navi_Link` { map, id, kind, .., name, .., x, y, to map, to x, to y },
/// `Navi_Npc` { map, id, kind, sprite, name, .., x, y }, `Navi_Mob` { map, id, kind, monster (low 16 bits), name, .. }.
fn navi_tables(lua: &Lua) -> io::Result<NaviTables> {
    let rows = |name: &str| -> io::Result<Vec<mlua::Table>> {
        let table: mlua::Table = lua.globals().get(name).map_err(lua_error)?;
        table.pairs::<mlua::Value, mlua::Table>().map(|pair| pair.map(|(_, row)| row).map_err(lua_error)).collect()
    };
    let text = |row: &mlua::Table, index: i64| row.get::<mlua::String>(index).map(|text| decode(&text)).unwrap_or_default();
    let number = |row: &mlua::Table, index: i64| row.get::<i64>(index).unwrap_or(0);
    let cell = |row: &mlua::Table, x: i64| [number(row, x) as u16, number(row, x + 1) as u16];
    let mut tables = NaviTables::default();
    for row in rows("Navi_Map")? {
        tables.maps.insert(text(&row, 1), text(&row, 2));
    }
    for row in rows("Navi_Link")? {
        tables.links.push(NaviLink {
            map: text(&row, 1),
            kind: number(&row, 3) as u32,
            name: text(&row, 5),
            cell: cell(&row, 7),
            to: text(&row, 9),
            to_cell: cell(&row, 10),
        });
    }
    for row in rows("Navi_Npc")? {
        tables.npcs.push(NaviNpc { map: text(&row, 1), name: text(&row, 5), cell: cell(&row, 7) });
    }
    for row in rows("Navi_Mob")? {
        tables.monsters.push(NaviMonster { map: text(&row, 1), id: (number(&row, 4) & 0xFFFF) as u16, name: text(&row, 5) });
    }
    Ok(tables)
}

pub fn entry_queue(grf: &Grf) -> io::Result<Vec<Battleground>> {
    battlegrounds(&run(grf, &[ENTRY_QUEUE])?)
}

/// `EntryQueueList` in order.
fn battlegrounds(lua: &Lua) -> io::Result<Vec<Battleground>> {
    let list: mlua::Table = lua.globals().get("EntryQueueList").map_err(lua_error)?;
    let mut entries: Vec<(i64, Battleground)> = Vec::new();
    for pair in list.pairs::<i64, mlua::Table>() {
        let (index, entry) = pair.map_err(lua_error)?;
        let text = |key: &str| entry.get::<mlua::String>(key).map(|text| decode(&text)).unwrap_or_default();
        let number = |key: &str| entry.get::<u32>(key).unwrap_or(0);
        entries.push((
            index,
            Battleground {
                name: text("BattleFieldName"),
                display: text("DisplayBattleFieldName"),
                sizes: [number("AllianceNum") as u16, number("EnemyNum") as u16],
                applies: [number("PrivateApply") == 1, number("PartyApply") == 1, number("GuildApply") == 1],
                condition: (number("EnterCondType") as u8, [number("EnterCondValue1"), number("EnterCondValue2")]),
                rewards: [text("RewardWin"), text("RewardDraw"), text("RewardLose")],
                goal: text("VictoryCond"),
                picture: text("MiniMapFile"),
            },
        ));
    }
    entries.sort_by_key(|(index, _)| *index);
    Ok(entries.into_iter().map(|(_, battleground)| battleground).collect())
}

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

/// The dress room's lists (`_DressHeadstyleList`, `_DressHeadPaletteList`, `_DressBodyPaletteList`): the values of
/// their `{label, value}` rows, in order.
pub fn dress_lists(grf: &Grf) -> io::Result<[Vec<u16>; 3]> {
    let lua = run(grf, &[DRESS_LIST, DRESS_F])?;
    let values = |name: &str| -> io::Result<Vec<u16>> {
        let table: mlua::Table = lua.globals().get(name).map_err(lua_error)?;
        let mut values = Vec::new();
        for row in table.sequence_values::<mlua::Table>() {
            values.push(row.map_err(lua_error)?.get::<u16>(2).map_err(lua_error)?);
        }
        Ok(values)
    };
    Ok([values("_DressHeadstyleList")?, values("_DressHeadPaletteList")?, values("_DressBodyPaletteList")?])
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

/// One row of the styling shop. `goods` is the look it gives (a color, a style, or a headgear's item id). The row's
/// number in its table is what the buy request sends; 0 in the request means that part stays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleRow {
    pub goods: u32,
    /// Zeny, when the row costs some.
    pub zeny: u32,
    /// The coupon it uses up, and the box that holds one. Either is enough; 0 is none.
    pub item: u32,
    pub box_item: u32,
    pub is_doram: Option<bool>,
    /// A hair style that can be dyed. Absent means the table doesn't say; false refuses a hair color.
    pub is_coloring: Option<bool>,
}

/// The styling shop's lists, by row number. Costume change is left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StylingShop {
    pub head_palette: BTreeMap<u16, StyleRow>,
    pub head_style: BTreeMap<u16, StyleRow>,
    pub doram_head_style: BTreeMap<u16, StyleRow>,
    pub body_palette: BTreeMap<u16, StyleRow>,
    pub doram_body_palette: BTreeMap<u16, StyleRow>,
    pub top_accessory: BTreeMap<u16, StyleRow>,
    pub mid_accessory: BTreeMap<u16, StyleRow>,
    pub bottom_accessory: BTreeMap<u16, StyleRow>,
    pub body_style: BTreeMap<u16, StyleRow>,
}

/// `StylingShop` after running the files' bytes in order (the framework, then the lists).
pub fn styling_shop(sources: &[Vec<u8>]) -> io::Result<StylingShop> {
    let lua = new_lua()?;
    for (index, source) in sources.iter().enumerate() {
        exec_bytes(&lua, &format!("stylingshop{index}"), source.clone())?;
    }
    let shop: mlua::Table = lua.globals().get("StylingShop").map_err(lua_error)?;
    let rows = |name: &str| style_rows(&shop, name);
    Ok(StylingShop {
        head_palette: rows("HeadPalette")?,
        head_style: rows("HeadStyle")?,
        doram_head_style: rows("DoramHeadStyle")?,
        body_palette: rows("BodyPalette")?,
        doram_body_palette: rows("DoramBodyPalette")?,
        top_accessory: rows("TopAccessory")?,
        mid_accessory: rows("MidAccessory")?,
        bottom_accessory: rows("BottomAccessory")?,
        body_style: rows("BodyStyle")?,
    })
}

/// The styling shop from data.grf (framework, then the lists).
pub fn load_styling_shop(grf: &Grf) -> io::Result<StylingShop> {
    styling_shop(&[grf.read(STYLING_SHOP_F)?, grf.read(STYLING_SHOP)?])
}

fn style_rows(shop: &mlua::Table, name: &str) -> io::Result<BTreeMap<u16, StyleRow>> {
    let Ok(Some(table)) = shop.get::<Option<mlua::Table>>(name) else { return Ok(BTreeMap::new()) };
    let number = |entry: &mlua::Table, key: &str| entry.get::<Option<u32>>(key).ok().flatten().unwrap_or(0);
    let flag = |entry: &mlua::Table, key: &str| entry.get::<Option<bool>>(key).ok().flatten();
    let mut rows = BTreeMap::new();
    for pair in table.pairs::<i64, mlua::Table>() {
        let (index, entry) = pair.map_err(lua_error)?;
        let Ok(index) = u16::try_from(index) else { continue };
        if index == 0 {
            continue;
        }
        let payment: Option<mlua::Table> = entry.get("payment").ok().flatten();
        rows.insert(
            index,
            StyleRow {
                goods: number(&entry, "goods"),
                zeny: payment.as_ref().map(|payment| number(payment, "zeny")).unwrap_or(0),
                item: payment.as_ref().map(|payment| number(payment, "itid")).unwrap_or(0),
                box_item: payment.as_ref().map(|payment| number(payment, "boxitid")).unwrap_or(0),
                is_doram: flag(&entry, "isDoram"),
                is_coloring: flag(&entry, "isColoring"),
            },
        );
    }
    Ok(rows)
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

/// Every item the client knows, by item ID (`tbl` in iteminfo): the client's own table, with a translation's entries
/// over it (a translation older than the data lacks the newer items, which keep the client's names).
pub fn item_info(grf: &Grf) -> io::Result<HashMap<u32, ItemInfo>> {
    let own = run(grf, &[ITEM_INFO]).and_then(|lua| items_in(&lua));
    let translated = TRANSLATED_ITEM_INFO.iter().find_map(|path| run(grf, &[path]).and_then(|lua| items_in(&lua)).ok());
    layered(own, translated)
}

/// The client's items with a translation's over them; either alone when the other is missing.
fn layered(own: io::Result<HashMap<u32, ItemInfo>>, translated: Option<HashMap<u32, ItemInfo>>) -> io::Result<HashMap<u32, ItemInfo>> {
    match (own, translated) {
        (Ok(mut items), Some(translated)) => {
            items.extend(translated);
            Ok(items)
        }
        (Ok(items), None) => Ok(items),
        (Err(_), Some(translated)) => Ok(translated),
        (Err(error), None) => Err(error),
    }
}

/// The items of a run iteminfo (`tbl`).
fn items_in(lua: &Lua) -> io::Result<HashMap<u32, ItemInfo>> {
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
    /// The level to use can be picked (`bSeperateLv`): the skill window's arrows, the wheel while aiming.
    pub separate_level: bool,
    /// Its range in cells by level (`AttackRange`), from level 1.
    pub attack_range: Vec<u16>,
}

/// The quests' titles, descriptions and summaries (`QuestInfoList`), from the translation's System folder.
const QUEST_INFO: &str = "system\\OngoingQuestInfoList_True.lub";

/// A quest's words, and the NPC cell the big minimap marks when the table gives one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestInfo {
    pub title: String,
    pub description: String,
    pub summary: String,
    /// ponytail: `NpcPosX`/`NpcPosY` on the quest itself. The real list can nest several NPCs, each with a sprite.
    pub npc: Option<[i32; 2]>,
    /// Only a cooldown timer (`CoolTimeQuest = 1`).
    pub cooldown: bool,
}

/// A quest's words: title, description (its lines joined), summary, its NPC cell when listed, whether it's a cooldown
/// timer.
pub fn quest_info(grf: &Grf) -> io::Result<HashMap<u32, QuestInfo>> {
    quest_infos(&run(grf, &[QUEST_INFO])?)
}

fn quest_infos(lua: &Lua) -> io::Result<HashMap<u32, QuestInfo>> {
    let list: mlua::Table = lua.globals().get("QuestInfoList").map_err(lua_error)?;
    let mut quests = HashMap::new();
    for pair in list.pairs::<i64, mlua::Table>() {
        let Ok((id, entry)) = pair else { continue };
        let text = |key: &str| entry.get::<mlua::String>(key).map(|text| decode(&text)).unwrap_or_default();
        let description = entry
            .get::<mlua::Table>("Description")
            .map(|lines| lines.sequence_values::<mlua::String>().flatten().map(|line| decode(&line)).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let npc = match (entry.get::<i64>("NpcPosX"), entry.get::<i64>("NpcPosY")) {
            (Ok(x), Ok(y)) => Some([x as i32, y as i32]),
            _ => None,
        };
        if let Ok(id) = u32::try_from(id) {
            let cooldown = entry.get::<i64>("CoolTimeQuest").unwrap_or(0) == 1;
            quests.insert(id, QuestInfo { title: text("Title"), description, summary: text("Summary"), npc, cooldown });
        }
    }
    Ok(quests)
}

/// Skills' descriptions by ID, line by line as the data has them.
pub fn skill_descriptions(grf: &Grf) -> io::Result<HashMap<u16, Vec<String>>> {
    skill_descripts(&run(grf, &[SKILL_ID, SKILL_DESCRIPT])?)
}

fn skill_descripts(lua: &Lua) -> io::Result<HashMap<u16, Vec<String>>> {
    let list: mlua::Table = lua.globals().get("SKILL_DESCRIPT").map_err(lua_error)?;
    let mut descriptions = HashMap::new();
    for pair in list.pairs::<i64, mlua::Table>() {
        let Ok((id, lines)) = pair else { continue };
        if let Ok(id) = u16::try_from(id) {
            descriptions.insert(id, lines.sequence_values::<mlua::String>().flatten().map(|line| decode(&line)).collect());
        }
    }
    Ok(descriptions)
}

pub fn skill_info(grf: &Grf) -> io::Result<HashMap<u16, SkillInfo>> {
    let lua = run(grf, &[JOB_INHERIT, SKILL_ID, SKILL_INFO])?;
    let list: mlua::Table = lua.globals().get("SKILL_INFO_LIST").map_err(lua_error)?;
    let mut skills = HashMap::new();
    for pair in list.pairs::<i64, mlua::Table>() {
        let Ok((id, entry)) = pair else { continue };
        let text = |value: mlua::Result<mlua::String>| value.map(|text| decode(&text)).unwrap_or_default();
        let info = SkillInfo {
            resource: text(entry.get(1)),
            name: text(entry.get("SkillName")),
            max_level: entry.get("MaxLv").unwrap_or(0),
            separate_level: entry.get("bSeperateLv").unwrap_or(false),
            attack_range: entry.get::<mlua::Table>("AttackRange").map(|ranges| ranges.sequence_values::<u16>().flatten().collect()).unwrap_or_default(),
        };
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

/// What a status icon says when pointed at: its lines, each with its colour (none: plain), one of them "%s" for the
/// time left when the effect has a time limit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateInfo {
    pub lines: Vec<(String, Option<[u8; 3]>)>,
    pub time_limit: bool,
}

pub fn state_info(grf: &Grf) -> io::Result<HashMap<u16, StateInfo>> {
    state_infos(&run(grf, &[EFST_IDS, STATE_ICON_INFO])?)
}

/// `StateIconList` by EFST.
fn state_infos(lua: &Lua) -> io::Result<HashMap<u16, StateInfo>> {
    let list: mlua::Table = lua.globals().get("StateIconList").map_err(lua_error)?;
    let mut infos = HashMap::new();
    for pair in list.pairs::<i64, mlua::Table>() {
        let Ok((efst, entry)) = pair else { continue };
        let Ok(descript) = entry.get::<mlua::Table>("descript") else { continue };
        let lines = descript
            .sequence_values::<mlua::Table>()
            .flatten()
            .map(|line| {
                let text = line.get::<mlua::String>(1).map(|text| decode(&text)).unwrap_or_default();
                let color =
                    line.get::<mlua::Table>(2).ok().and_then(|color| Some([color.get::<u8>(1).ok()?, color.get::<u8>(2).ok()?, color.get::<u8>(3).ok()?]));
                (text, color)
            })
            .collect();
        if let Ok(efst) = u16::try_from(efst) {
            infos.insert(efst, StateInfo { lines, time_limit: entry.get::<i64>("haveTimeLimit").unwrap_or(0) == 1 });
        }
    }
    Ok(infos)
}

/// Sprite scales by monster job. A job in neither file is absent, and drawn at 1×. The new file wins where both
/// list the job. A file that fails to parse is skipped; the other still counts.
pub fn monster_sizes(grf: &Grf) -> HashMap<u16, f32> {
    let mut sizes = HashMap::new();
    for path in [MONSTER_SIZE, MONSTER_SIZE_NEW] {
        match grf.read(path) {
            Ok(bytes) => {
                if let Err(error) = merge_monster_sizes(&mut sizes, &bytes) {
                    eprintln!("monster sizes ({path}): {error}");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("monster sizes ({path}): {error}"),
        }
    }
    sizes
}

/// How wide each sprite job's shadow is, in cells (1 when the job isn't listed): a unit standing on a radius of r
/// cells (`NpcLocationRadius`) gets 2r + 1, else the shadow table's factor (`ShadowFactorTable`, 0 for none).
pub fn shadow_factors(grf: &Grf) -> io::Result<HashMap<u16, f32>> {
    let read = |path: &str| grf.read(path);
    shadow_tables(read(NPC_IDENTITY)?, read(JOB_IDENTITY)?, read(SHADOW_TABLE)?, read(NPC_LOCATION_RADIUS).ok())
}

fn shadow_tables(npc_identity: Vec<u8>, job_identity: Vec<u8>, shadows: Vec<u8>, radius: Option<Vec<u8>>) -> io::Result<HashMap<u16, f32>> {
    let lua = new_lua()?;
    exec_bytes(&lua, NPC_IDENTITY, npc_identity)?;
    exec_bytes(&lua, JOB_IDENTITY, job_identity)?;
    // Names these identity tables don't know all land on one unused key.
    lua.load("for _, t in ipairs({jobtbl, JTtbl}) do setmetatable(t, { __index = function() return -1 end }) end").exec().map_err(lua_error)?;
    exec_bytes(&lua, SHADOW_TABLE, shadows)?;
    let mut factors = HashMap::new();
    let table = |name: &str| lua.globals().get::<mlua::Table>(name).map_err(lua_error);
    for pair in table("ShadowFactorTable")?.pairs::<mlua::Value, mlua::Value>() {
        let (job, factor) = pair.map_err(lua_error)?;
        if let (Some(job), Some(factor)) = (number(&job).and_then(|job| u16::try_from(job).ok()), lua_f32(factor))
            && factor >= 0.0
        {
            factors.insert(job, factor);
        }
    }
    if let Some(radius) = radius {
        exec_bytes(&lua, NPC_LOCATION_RADIUS, radius)?;
        for pair in table("NpcLocationRadius")?.pairs::<mlua::Value, mlua::Value>() {
            let (job, radius) = pair.map_err(lua_error)?;
            if let (Some(job), Some(radius)) = (number(&job).and_then(|job| u16::try_from(job).ok()), lua_f32(radius))
                && radius != 0.0
            {
                factors.insert(job, 2.0 * radius + 1.0);
            }
        }
    }
    Ok(factors)
}

/// The old table, then the new one over it. A missing side is skipped.
#[cfg(test)]
fn monster_size_tables(old: Option<&[u8]>, new: Option<&[u8]>) -> io::Result<HashMap<u16, f32>> {
    let mut sizes = HashMap::new();
    for bytes in [old, new].into_iter().flatten() {
        merge_monster_sizes(&mut sizes, bytes)?;
    }
    Ok(sizes)
}

/// `tbl[job].MonsterSize` from one size-effect file, written over `sizes`.
fn merge_monster_sizes(sizes: &mut HashMap<u16, f32>, bytes: &[u8]) -> io::Result<()> {
    let lua = new_lua()?;
    exec_bytes(&lua, "monster_size_effect", bytes.to_vec())?;
    let tbl: mlua::Table = lua.globals().get("tbl").map_err(lua_error)?;
    for pair in tbl.pairs::<mlua::Value, mlua::Value>() {
        let (key, value) = pair.map_err(lua_error)?;
        let (Some(job), mlua::Value::Table(info)) = (number(&key).and_then(|id| u16::try_from(id).ok()), value) else { continue };
        let Some(size) = info.get::<mlua::Value>("MonsterSize").ok().and_then(lua_f32) else { continue };
        sizes.insert(job, size);
    }
    Ok(())
}

fn lua_f32(value: mlua::Value) -> Option<f32> {
    match value {
        mlua::Value::Integer(n) => Some(n as f32),
        mlua::Value::Number(n) => Some(n as f32),
        _ => None,
    }
}

/// How big a unit is drawn: the size table's factor (1 when the job is not listed), times the ChangeSize percent
/// (100 keeps the table's size) when that status is on.
pub fn sprite_scale(table_factor: f32, change_percent: Option<i32>) -> f32 {
    match change_percent {
        Some(percent) => table_factor * percent as f32 / 100.0,
        None => table_factor,
    }
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

const ENCHANT_LIST_F: &str = "data\\luafiles514\\lua files\\enchant\\enchantlist_f.lub";
const ENCHANT_LIST: &str = "data\\luafiles514\\lua files\\enchant\\enchantlist.lub";
pub const ITEM_DB_NAMES: &str = "data\\luafiles514\\lua files\\itemdbnametbl.lub";
// ponytail: the official client provides these; the values are what lets every table load. LoadAllData is not called
// (it hands the tables to the client). The C_Set stubs are there in case a file calls it anyway.
const ENCHANT_STUBS: &str = r#"
MAX_SLOT_NUM = 4
MAX_REFINE_LEVEL = 20
MAX_GRADE_LEVEL = 4
MAX_MATERIAL_NUM = 10
function C_GetSlotCount() return 0 end
function MessageBox() end
function C_SetSlotOrder() return true, "" end
function C_AddTargetItem() return true, "" end
function C_SetCondition() return true, "" end
function C_ApproveRandomOption() return true, "" end
function C_SetReset() return true, "" end
function C_SetCaution() return true, "" end
function C_SetRequire() return true, "" end
function C_SetSuccessRate() return true, "" end
function C_SetGradeBonus() return true, "" end
function C_SetEnchant() return true, "" end
function C_AddPerfectEnchant() return true, "" end
function C_AddUpgradeEnchant() return true, "" end
"#;

/// What a random or perfect enchant, an upgrade or a reset costs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnchantCost {
    pub zeny: u32,
    pub materials: Vec<(u32, u32)>,
}

/// A perfect enchant: the item it puts in, and what it costs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerfectEnchant {
    pub item: u32,
    pub cost: EnchantCost,
}

/// An upgrade of an enchant already in a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnchantUpgrade {
    pub result: u32,
    pub cost: EnchantCost,
}

/// One card slot of an enchant table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnchantSlot {
    pub cost: EnchantCost,
    pub success_rate: u32,
    /// Added to the success rate, by grade.
    pub grade_bonus: Vec<(u8, u32)>,
    /// Random enchants at a grade: item id and weight, out of 100000.
    pub rates: Vec<(u8, Vec<(u32, u32)>)>,
    pub perfect: Vec<PerfectEnchant>,
    /// The enchant in the slot, and what it upgrades to.
    pub upgrades: Vec<(u32, EnchantUpgrade)>,
}

impl EnchantSlot {
    pub fn bonus(&self, grade: u8) -> u32 {
        self.grade_bonus.iter().find(|(listed, _)| *listed == grade).map(|(_, bonus)| *bonus).unwrap_or(0)
    }

    pub fn rate_list(&self, grade: u8) -> &[(u32, u32)] {
        self.rates.iter().find(|(listed, _)| *listed == grade).map(|(_, list)| list.as_slice()).unwrap_or(&[])
    }

    /// Random enchants are offered when the slot has a rate table at all.
    pub fn random(&self) -> bool {
        !self.rates.is_empty()
    }
}

/// Resetting the enchants on an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnchantReset {
    pub enabled: bool,
    /// Out of 100000. Zero is not offered: rathena takes the cost and then doesn't answer.
    pub rate: u32,
    pub cost: EnchantCost,
}

/// One enchant table (`Table[n]` in the client's enchant list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnchantTable {
    pub targets: Vec<u32>,
    pub slot_order: Vec<u8>,
    pub min_refine: u8,
    pub min_grade: u8,
    /// Whether an item with random options may go in. The client doesn't track those, so this is recorded only.
    pub approve_random_opt: bool,
    pub reset: Option<EnchantReset>,
    /// Card slots 0 to 3.
    pub slots: [Option<EnchantSlot>; 4],
}

/// The enchant tables from data.grf (`enchantlist_f`, `enchantlist`, `itemdbnametbl`).
pub fn enchants(grf: &Grf) -> io::Result<HashMap<u64, EnchantTable>> {
    let names = grf.read(ITEM_DB_NAMES).unwrap_or_default();
    let mut lists = Vec::new();
    if let Ok(bytes) = grf.read(ENCHANT_LIST_F) {
        lists.push(bytes);
    }
    lists.push(grf.read(ENCHANT_LIST)?);
    enchant_tables(&lists, names)
}

/// Reads `Table` from the list files' bytes (the `_f` file first, when there is one) and item names from `names`.
/// A test passes a small Lua source of the same shape. Names are resolved by the raw bytes of the key.
pub fn enchant_tables(lists: &[Vec<u8>], names: Vec<u8>) -> io::Result<HashMap<u64, EnchantTable>> {
    let lua = new_lua()?;
    lua.load(ENCHANT_STUBS).set_name("enchant stubs").exec().map_err(lua_error)?;
    if !names.is_empty() {
        exec_bytes(&lua, ITEM_DB_NAMES, names)?;
    }
    for (index, bytes) in lists.iter().enumerate() {
        exec_bytes(&lua, &format!("enchant list {index}"), bytes.clone())?;
    }
    let names = loaded_item_names(&lua);
    let table: mlua::Table = lua.globals().get("Table").map_err(lua_error)?;
    let mut tables = HashMap::new();
    for pair in table.pairs::<mlua::Value, mlua::Value>() {
        let (key, value) = pair.map_err(lua_error)?;
        let (Some(id), mlua::Value::Table(entry)) = (lua_u64(&key), value) else { continue };
        tables.insert(id, read_enchant(&entry, &names));
    }
    Ok(tables)
}

/// `ItemDBNameTbl`, keyed by the Lua string's own bytes (Korean names are cp949, not UTF-8).
fn loaded_item_names(lua: &Lua) -> HashMap<Vec<u8>, u32> {
    let Ok(table) = lua.globals().get::<mlua::Table>("ItemDBNameTbl") else { return HashMap::new() };
    let mut names = HashMap::new();
    for pair in table.pairs::<mlua::Value, mlua::Value>().flatten() {
        if let (mlua::Value::String(key), Some(id)) = (pair.0, value_u32(&pair.1)) {
            names.insert(key.as_bytes().as_ref().to_vec(), id);
        }
    }
    names
}

fn value_u32(value: &mlua::Value) -> Option<u32> {
    match value {
        mlua::Value::Integer(id) => u32::try_from(*id).ok(),
        mlua::Value::Number(id) if id.is_finite() && *id >= 0.0 && *id <= u32::MAX as f64 => Some(*id as u32),
        _ => None,
    }
}

fn lua_u64(value: &mlua::Value) -> Option<u64> {
    match value {
        mlua::Value::Integer(id) => u64::try_from(*id).ok(),
        mlua::Value::Number(id) if id.is_finite() && *id >= 0.0 => Some(*id as u64),
        _ => None,
    }
}

fn named_id(value: &mlua::Value, names: &HashMap<Vec<u8>, u32>) -> Option<u32> {
    match value {
        mlua::Value::String(text) => {
            let bytes = text.as_bytes();
            names.get(bytes.as_ref()).copied()
        }
        _ => value_u32(value),
    }
}

fn mat_list(table: &mlua::Table, names: &HashMap<Vec<u8>, u32>) -> Vec<(u32, u32)> {
    let Ok(mats) = table.get::<mlua::Table>("MatTbl") else { return Vec::new() };
    let mut list = Vec::new();
    for (key, amount) in mats.pairs::<mlua::Value, mlua::Value>().flatten() {
        if let (Some(id), Some(amount)) = (named_id(&key, names), value_u32(&amount)) {
            list.push((id, amount));
        }
    }
    list.sort();
    list
}

fn cost_of(table: &mlua::Table, names: &HashMap<Vec<u8>, u32>) -> EnchantCost {
    EnchantCost { zeny: table.get::<u32>("Zeny").unwrap_or(0), materials: mat_list(table, names) }
}

fn read_enchant(entry: &mlua::Table, names: &HashMap<Vec<u8>, u32>) -> EnchantTable {
    let targets = entry
        .get::<mlua::Table>("TargetItemTbl")
        .map(|items| items.sequence_values::<mlua::Value>().flatten().filter_map(|name| named_id(&name, names)).collect())
        .unwrap_or_default();
    let slot_order =
        entry.get::<mlua::Table>("SlotOrder").map(|order| order.sequence_values::<u8>().flatten().filter(|slot| *slot < 4).collect()).unwrap_or_default();
    let condition = entry.get::<mlua::Table>("Condition").ok();
    let number = |table: &mlua::Table, key: &str| table.get::<u32>(key).unwrap_or(0);
    let reset = entry.get::<mlua::Table>("Reset").ok().map(|reset| EnchantReset {
        enabled: lua_flag(&reset, "bReset"),
        rate: number(&reset, "Rate"),
        cost: cost_of(&reset, names),
    });
    let mut slots: [Option<EnchantSlot>; 4] = [None, None, None, None];
    if let Ok(table) = entry.get::<mlua::Table>("Slot") {
        for (key, slot) in table.pairs::<mlua::Value, mlua::Table>().flatten() {
            let Some(index) = value_u32(&key).filter(|index| *index < 4) else { continue };
            slots[index as usize] = Some(read_slot(&slot, names));
        }
    }
    EnchantTable {
        targets,
        slot_order,
        min_refine: condition.as_ref().map(|condition| number(condition, "MinRefine") as u8).unwrap_or(0),
        min_grade: condition.as_ref().map(|condition| number(condition, "MinGrade") as u8).unwrap_or(0),
        approve_random_opt: lua_flag(entry, "bApproveRandomOpt"),
        reset,
        slots,
    }
}

fn read_slot(slot: &mlua::Table, names: &HashMap<Vec<u8>, u32>) -> EnchantSlot {
    let require = slot.get::<mlua::Table>("RequireTbl").ok();
    let mut grade_bonus = Vec::new();
    if let Ok(bonuses) = slot.get::<mlua::Table>("GradeBonusTbl") {
        for (grade, bonus) in bonuses.pairs::<mlua::Value, mlua::Value>().flatten() {
            if let (Some(grade), Some(bonus)) = (value_u32(&grade), value_u32(&bonus)) {
                grade_bonus.push((grade as u8, bonus));
            }
        }
        grade_bonus.sort();
    }
    let mut rates = Vec::new();
    if let Ok(table) = slot.get::<mlua::Table>("EnchantRateTbl") {
        for (grade, list) in table.pairs::<mlua::Value, mlua::Table>().flatten() {
            let Some(grade) = value_u32(&grade) else { continue };
            let mut enchants = Vec::new();
            for (name, weight) in list.pairs::<mlua::Value, mlua::Value>().flatten() {
                if let (Some(id), Some(weight)) = (named_id(&name, names), value_u32(&weight)) {
                    enchants.push((id, weight));
                }
            }
            enchants.sort();
            rates.push((grade as u8, enchants));
        }
        rates.sort_by_key(|(grade, _)| *grade);
    }
    let mut perfect = Vec::new();
    if let Ok(table) = slot.get::<mlua::Table>("PerfectECTbl") {
        for (name, info) in table.pairs::<mlua::Value, mlua::Table>().flatten() {
            if let Some(item) = named_id(&name, names) {
                perfect.push(PerfectEnchant { item, cost: cost_of(&info, names) });
            }
        }
        perfect.sort_by_key(|row| row.item);
    }
    let mut upgrades = Vec::new();
    if let Ok(table) = slot.get::<mlua::Table>("UpgradeECTbl") {
        for (name, info) in table.pairs::<mlua::Value, mlua::Table>().flatten() {
            let result = info.get::<mlua::Value>("ResultItemDB").ok().and_then(|name| named_id(&name, names));
            if let (Some(item), Some(result)) = (named_id(&name, names), result) {
                upgrades.push((item, EnchantUpgrade { result, cost: cost_of(&info, names) }));
            }
        }
        upgrades.sort_by_key(|(item, _)| *item);
    }
    EnchantSlot {
        cost: require.as_ref().map(|require| cost_of(require, names)).unwrap_or(EnchantCost { zeny: 0, materials: Vec::new() }),
        success_rate: slot.get::<u32>("SuccessRate").unwrap_or(0),
        grade_bonus,
        rates,
        perfect,
        upgrades,
    }
}

/// `ReformItemList` and `ReformInfo`.
pub const ITEM_REFORM: &str = "data\\luafiles514\\lua files\\itemreform\\itemreformsystem.lub";

/// One way a reform item can turn a carried item into another.
#[derive(Debug, Clone, PartialEq)]
pub struct ReformInfo {
    pub base: u32,
    pub result: u32,
    /// Material item and how many.
    pub materials: Vec<(u32, u32)>,
    pub refine_min: u16,
    pub refine_max: u16,
    /// Random options the item must have. The client doesn't track them, so the button ignores this.
    pub need_options: u8,
    /// The item may hold no cards.
    pub empty_socket: bool,
    /// Cards and enchants stay. False: they are removed.
    pub preserve_socket: bool,
    /// The item's grade stays. False: it is removed.
    pub preserve_grade: bool,
    /// Added to the refine (negative lowers it).
    pub change_refine: i32,
    pub random_option: String,
}

/// `ItemDBNameTbl` as name bytes to item id. Keys stay the Lua string's raw bytes: some names are cp949, not UTF-8.
/// A table stored as id to name is read the other way into the same map.
pub fn item_db_names(source: Vec<u8>) -> io::Result<HashMap<Vec<u8>, u32>> {
    let lua = new_lua()?;
    exec_bytes(&lua, ITEM_DB_NAMES, source)?;
    let table: mlua::Table = lua.globals().get("ItemDBNameTbl").map_err(lua_error)?;
    let mut names = HashMap::new();
    for pair in table.pairs::<mlua::Value, mlua::Value>() {
        let (key, value) = pair.map_err(lua_error)?;
        let id_of = |value: &mlua::Value| number(value).and_then(|id| u32::try_from(id).ok());
        if let (Some(name), Some(id)) = (raw_bytes(&key), id_of(&value)) {
            names.insert(name, id);
        } else if let (Some(id), Some(name)) = (id_of(&key), raw_bytes(&value)) {
            names.insert(name, id);
        }
    }
    Ok(names)
}

/// `ReformItemList` and `ReformInfo` from the file's bytes. Names resolve through `names`. `InformationString` is skipped.
pub fn reform_table(source: Vec<u8>, names: &HashMap<Vec<u8>, u32>) -> io::Result<HashMap<u32, Vec<ReformInfo>>> {
    let lua = new_lua()?;
    exec_bytes(&lua, ITEM_REFORM, source)?;
    let globals = lua.globals();
    let list: mlua::Table = globals.get("ReformItemList").map_err(lua_error)?;
    let info: mlua::Table = globals.get("ReformInfo").map_err(lua_error)?;
    let mut reforms = HashMap::new();
    for pair in list.pairs::<mlua::Value, mlua::Value>() {
        let (key, value) = pair.map_err(lua_error)?;
        let (Some(name), mlua::Value::Table(indexes)) = (raw_bytes(&key), value) else { continue };
        let Some(&item) = names.get(&name) else { continue };
        let mut entries = Vec::new();
        for index in entry_numbers(&indexes)? {
            let Ok(row) = info.get::<mlua::Table>(index) else { continue };
            if let Some(entry) = read_reform(&row, names) {
                entries.push(entry);
            }
        }
        if !entries.is_empty() {
            reforms.insert(item, entries);
        }
    }
    Ok(reforms)
}

fn raw_bytes(value: &mlua::Value) -> Option<Vec<u8>> {
    match value {
        mlua::Value::String(text) => Some(text.as_bytes().to_vec()),
        _ => None,
    }
}

/// Lua 5.1 stores every number as a double; only whole ones are taken.
fn number(value: &mlua::Value) -> Option<i64> {
    match value {
        mlua::Value::Integer(n) => Some(*n),
        mlua::Value::Number(n) if n.fract() == 0.0 => format!("{n:.0}").parse().ok(),
        _ => None,
    }
}

/// The list's entry numbers, in the order written.
fn entry_numbers(table: &mlua::Table) -> io::Result<Vec<i64>> {
    let mut numbers = Vec::new();
    for index in 1..=table.raw_len() {
        if let Ok(n) = table.get::<i64>(index) {
            numbers.push(n);
        }
    }
    if numbers.is_empty() {
        for pair in table.pairs::<mlua::Value, mlua::Value>() {
            let (_, value) = pair.map_err(lua_error)?;
            if let Some(n) = number(&value) {
                numbers.push(n);
            }
        }
    }
    Ok(numbers)
}

fn field_number(table: &mlua::Table, key: &str) -> Option<i64> {
    table.get::<mlua::Value>(key).ok().as_ref().and_then(number)
}

fn flag(table: &mlua::Table, key: &str, default: bool) -> bool {
    match table.get::<mlua::Value>(key) {
        Ok(mlua::Value::Boolean(value)) => value,
        Ok(value) => number(&value).map(|n| n != 0).unwrap_or(default),
        Err(_) => default,
    }
}

fn reform_named(table: &mlua::Table, key: &str, names: &HashMap<Vec<u8>, u32>) -> Option<u32> {
    let value = table.get::<mlua::Value>(key).ok()?;
    names.get(&raw_bytes(&value)?).copied()
}

fn read_reform(entry: &mlua::Table, names: &HashMap<Vec<u8>, u32>) -> Option<ReformInfo> {
    let base = reform_named(entry, "BaseItem", names)?;
    let result = reform_named(entry, "ResultItem", names)?;
    let mut materials = Vec::new();
    if let Ok(table) = entry.get::<mlua::Table>("Material") {
        for pair in table.pairs::<mlua::Value, mlua::Value>().flatten() {
            if let (Some(name), Some(amount)) = (raw_bytes(&pair.0), number(&pair.1))
                && let Some(&id) = names.get(&name)
            {
                materials.push((name, id, u32::try_from(amount).unwrap_or(0)));
            }
        }
        materials.sort_by(|a, b| a.0.cmp(&b.0));
    }
    let narrow = |n: i64| u16::try_from(n).unwrap_or(if n < 0 { 0 } else { u16::MAX });
    Some(ReformInfo {
        base,
        result,
        materials: materials.into_iter().map(|(_, id, amount)| (id, amount)).collect(),
        refine_min: field_number(entry, "NeedRefineMin").map(narrow).unwrap_or(0),
        refine_max: field_number(entry, "NeedRefineMax").map(narrow).unwrap_or(u16::MAX),
        need_options: u8::try_from(field_number(entry, "NeedOptionNumMin").unwrap_or(0)).unwrap_or(u8::MAX),
        empty_socket: flag(entry, "IsEmptySocket", false),
        preserve_socket: flag(entry, "PreserveSocketItem", true),
        preserve_grade: flag(entry, "PreserveGrade", true),
        change_refine: i32::try_from(field_number(entry, "ChangeRefineValue").unwrap_or(0)).unwrap_or(0),
        random_option: entry.get::<mlua::String>("RandomOptionCode").ok().map(|text| decode(&text)).unwrap_or_default(),
    })
}

/// `data\luafiles514\lua files\hotkey_v2.lub`: the game's shortcut defaults.
const HOTKEY_V2: &str = "data\\luafiles514\\lua files\\hotkey_v2.lub";

/// One row of `HOTKEY_<n>` or `USERKEY_<n>`: the action name and the virtual-key codes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeyRow {
    /// 1 hotbar, 2 interface, 3 macros and flags, 4 the second hotbar.
    pub table: u8,
    pub index: u16,
    pub exe: String,
    /// Missing, or 0, when the row is unbound.
    pub key1: Option<u16>,
    pub key2: u16,
    /// A row from `USERKEY_<n>`, which replaces the default at the same table and index.
    pub user: bool,
}

/// The shortcut tables in `grf`'s hotkey_v2.lub.
pub fn hotkey_defaults(grf: &Grf) -> io::Result<Vec<HotkeyRow>> {
    hotkey_bytes(&grf.read(HOTKEY_V2)?)
}

/// Shortcut rows from a Lua source in the same shape as hotkey_v2.lub or UserKeys.lua.
pub fn hotkey_rows(source: &str) -> io::Result<Vec<HotkeyRow>> {
    hotkey_bytes(source.as_bytes())
}

/// `MsgStrID.MSI_HK_…` in older tables is the name itself, so a missing message table still yields the EXE string.
const MSGSTRID: &str = "MsgStrID = setmetatable({}, { __index = function(_, name) return name end })";

fn hotkey_bytes(bytes: &[u8]) -> io::Result<Vec<HotkeyRow>> {
    let lua = new_lua()?;
    lua.load(MSGSTRID).exec().map_err(lua_error)?;
    exec_bytes(&lua, HOTKEY_V2, bytes.to_vec())?;
    let mut rows = Vec::new();
    for table in 1..=4 {
        rows.extend(hotkey_table(&lua, &format!("HOTKEY_{table}"), table, false)?);
        rows.extend(hotkey_table(&lua, &format!("USERKEY_{table}"), table, true)?);
    }
    Ok(rows)
}

fn hotkey_table(lua: &Lua, name: &str, table: u8, user: bool) -> io::Result<Vec<HotkeyRow>> {
    let Ok(entries) = lua.globals().get::<mlua::Table>(name) else { return Ok(Vec::new()) };
    let mut rows = Vec::new();
    for pair in entries.pairs::<mlua::Value, mlua::Value>() {
        let (key, value) = pair.map_err(lua_error)?;
        let (Some(index), mlua::Value::Table(entry)) = (number(&key).and_then(|index| u16::try_from(index).ok()), value) else { continue };
        let Some(exe) = entry.get::<mlua::String>("EXE").ok().map(|text| decode(&text)).filter(|exe| !exe.is_empty() && exe != "NULL") else {
            continue;
        };
        // ponytail: KEY1 0 is unbound, the same as leaving it out (the save file omits a 0).
        let key1 = field_number(&entry, "KEY1").and_then(|code| u16::try_from(code).ok()).filter(|code| *code > 0);
        let key2 = field_number(&entry, "KEY2").and_then(|code| u16::try_from(code).ok()).unwrap_or(0);
        rows.push(HotkeyRow { table, index, exe, key1, key2, user });
    }
    rows.sort_by_key(|row| row.index);
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use crate::grf::Grf;

    #[test]
    fn reads_navigation_tables() {
        let lua = super::new_lua().unwrap();
        let source = r#"
            Navi_Map = { { "prontera", "Prontera", 5001, 312, 392 }, { "prt_fild08", "Prontera Field", 5002, 400, 400 } }
            Navi_Link = { { "prt_fild08", 100, 200, 99999, "warp", "", 170, 375, "prontera", 156, 22 } }
            Navi_Npc = { { "prontera", 7, 101, 706, "Tool Dealer", "yusa", 134, 221 } }
            Navi_Mob = { { "prt_fild08", 9, 300, 1377258, "Poring", "PORING", 60, 1 } }
        "#;
        super::exec_bytes(&lua, "navi", source.as_bytes().to_vec()).unwrap();
        let tables = super::navi_tables(&lua).unwrap();
        assert_eq!(tables.maps.get("prt_fild08").map(String::as_str), Some("Prontera Field"));
        let link = super::NaviLink { map: "prt_fild08".into(), kind: 200, name: "warp".into(), cell: [170, 375], to: "prontera".into(), to_cell: [156, 22] };
        assert_eq!(tables.links, [link]);
        assert_eq!(tables.npcs, [super::NaviNpc { map: "prontera".into(), name: "Tool Dealer".into(), cell: [134, 221] }]);
        assert_eq!(tables.monsters, [super::NaviMonster { map: "prt_fild08".into(), id: 1002, name: "Poring".into() }]);
    }

    #[test]
    fn lays_a_translation_over_the_clients_items() {
        let item = |name: &str| super::ItemInfo { name: name.into(), ..Default::default() };
        let own = [(501, item("빨간포션")), (25_000, item("새 아이템"))].into_iter().collect();
        let translated = [(501, item("Red Potion"))].into_iter().collect();
        let items = super::layered(Ok(own), Some(translated)).unwrap();
        // Translated where the translation has it; the newer item keeps the client's name.
        assert_eq!((items[&501].name.as_str(), items[&25_000].name.as_str()), ("Red Potion", "새 아이템"));
        assert!(super::layered(Err(std::io::ErrorKind::NotFound.into()), None).is_err());
    }

    #[test]
    fn reads_shadow_factors_by_sprite_job() {
        let npcs = b"jobtbl = { JT_WARPNPC = 45, JT_PORING = 1002, JT_KADES = 1900, JT_EGG = 1047 }".to_vec();
        let jobs = b"JTtbl = { JT_KNIGHT_H = 4014, JT_NOVICE = 0 }".to_vec();
        let shadows = br#"
            ShadowFactorTable = {
                [jobtbl.JT_WARPNPC] = 0, [jobtbl.JT_PORING] = 0.5, [jobtbl.JT_EGG] = 0,
                [JTtbl.JT_KNIGHT_H] = 1.5, [jobtbl.JT_NOT_IN_THIS_DATA] = 3, [jobtbl.JT_KADES] = 2,
            }
        "#
        .to_vec();
        let radius = b"NpcLocationRadius = { [jobtbl.JT_KADES] = 1 }".to_vec();
        let factors = super::shadow_tables(npcs.clone(), jobs.clone(), shadows.clone(), Some(radius)).unwrap();
        // None for warps and eggs, a smaller one for a Poring, a mounted Knight's wider.
        assert_eq!((factors[&45], factors[&1047], factors[&1002], factors[&4014]), (0.0, 0.0, 0.5, 1.5));
        // A unit standing on a radius of one cell: three cells wide, whatever the shadow table says.
        assert_eq!(factors[&1900], 3.0);
        // Unlisted jobs (the Novice) and names this data doesn't know aren't in the table.
        assert!(!factors.contains_key(&0));
        assert_eq!(factors.len(), 5);
        assert_eq!(super::shadow_tables(npcs, jobs, shadows, None).unwrap()[&1900], 2.0);
    }

    #[test]
    fn reads_monster_sizes_and_lets_the_new_table_win() {
        let old = br#"
            tbl = {
                [1031] = { MonsterSize = 1.2, MonsterEff = -1 },
                [1111] = { MonsterSize = 0.5, MonsterEff = { -1 } },
                [2222] = { MonsterSize = 2 },
            }
        "#;
        let new_table = br#"
            tbl = {
                [1031] = { MonsterSize = 1.5 },
                [3333] = { MonsterSize = 2.5 },
            }
        "#;
        let sizes = super::monster_size_tables(Some(old), Some(new_table)).unwrap();
        // Poring (1002) is in neither table.
        assert!(!sizes.contains_key(&1002));
        assert_eq!(super::sprite_scale(sizes.get(&1002).copied().unwrap_or(1.0), None), 1.0);
        // The new table replaces the old size. A job only the old table has stays.
        assert_eq!((sizes[&1031], sizes[&1111], sizes[&2222], sizes[&3333]), (1.5, 0.5, 2.0, 2.5));
        assert_eq!(super::monster_size_tables(Some(old), None).unwrap()[&1031], 1.2);
        assert_eq!(super::monster_size_tables(None, Some(new_table)).unwrap()[&3333], 2.5);
        // ChangeSize's percent multiplies the table factor. 100% leaves it.
        assert_eq!(super::sprite_scale(sizes[&1031], Some(150)), 2.25);
        assert_eq!(super::sprite_scale(sizes[&1031], Some(100)), 1.5);
        assert_eq!(super::sprite_scale(1.0, Some(50)), 0.5);
        assert_eq!(super::sprite_scale(1.2, None), 1.2);
    }

    #[test]
    fn reads_quest_info() {
        let lua = super::new_lua().unwrap();
        lua.load(
            r#"QuestInfoList = {
                [7001] = { Title = "Hunting Porings", Description = { "Hunt", "ten Porings." }, Summary = "Poring x10" },
                [7002] = { Title = "See Kafra", Description = { "Talk." }, Summary = "Kafra", NpcPosX = 150, NpcPosY = 180 },
                [7003] = { Title = "Wait", CoolTimeQuest = 1 },
            }"#,
        )
        .exec()
        .unwrap();
        let quests = super::quest_infos(&lua).unwrap();
        assert_eq!(quests[&7001].title, "Hunting Porings");
        assert_eq!(quests[&7001].description, "Hunt ten Porings.");
        assert_eq!(quests[&7001].npc, None);
        assert_eq!(quests[&7002].npc, Some([150, 180]));
        assert!(!quests[&7001].cooldown && quests[&7003].cooldown, "a cooldown timer");
    }

    #[test]
    fn reads_skill_descriptions() {
        let lua = super::new_lua().unwrap();
        lua.load(r#"SKID = { SM_BASH = 5 } SKILL_DESCRIPT = { [SKID.SM_BASH] = { "Bash", "Max Level: 10", "^777777Hits hard^000000" } }"#).exec().unwrap();
        let descriptions = super::skill_descripts(&lua).unwrap();
        assert_eq!(descriptions[&5], ["Bash", "Max Level: 10", "^777777Hits hard^000000"]);
    }

    #[test]
    fn reads_what_status_icons_say() {
        let lua = super::new_lua().unwrap();
        lua.load(
            r#"EFST_IDs = { EFST_BLESSING = 10, EFST_AUTOBERSERK = 107 }
            COLOR_TITLE_BUFF = { 155, 202, 155 }
            COLOR_TIME = { 255, 176, 98 }
            StateIconList = {}
            StateIconList[EFST_IDs.EFST_BLESSING] = { haveTimeLimit = 1, posTimeLimitStr = 2,
                descript = { { "Blessing", COLOR_TITLE_BUFF }, { "%s", COLOR_TIME }, { "Increases STR, DEX and INT" } } }
            StateIconList[EFST_IDs.EFST_AUTOBERSERK] = { descript = { { "Berserk", COLOR_TITLE_BUFF } } }"#,
        )
        .exec()
        .unwrap();
        let infos = super::state_infos(&lua).unwrap();
        let blessing = &infos[&10];
        assert!(blessing.time_limit);
        assert_eq!(
            blessing.lines,
            [("Blessing".into(), Some([155, 202, 155])), ("%s".into(), Some([255, 176, 98])), ("Increases STR, DEX and INT".into(), None)]
        );
        assert!(!infos[&107].time_limit);
    }

    #[test]
    fn reads_the_battleground_list() {
        let lua = super::new_lua().unwrap();
        lua.load(
            r#"EntryQueueList = {
                [2] = { BattleFieldName = "KVM (Level 80 and up)", DisplayBattleFieldName = "KVM", AllianceNum = 5, EnemyNum = 5,
                    PrivateApply = 1, PartyApply = 1, GuildApply = 0, EnterCondType = 2, EnterCondValue1 = 80, EnterCondValue2 = 0,
                    RewardWin = "5 Badges", RewardDraw = "2", RewardLose = "1", VictoryCond = "More kills", MiniMapFile = "bat_c01.bmp" },
                [1] = { BattleFieldName = "Tierra Gorge", AllianceNum = 6, EnemyNum = 6, EnterCondType = 3, EnterCondValue1 = 60,
                    EnterCondValue2 = 79 },
            }
            function ReadEntryQueueList() for k, v in pairs(EntryQueueList) do AddEntryQueue(k, v) end end"#,
        )
        .exec()
        .unwrap();
        let list = super::battlegrounds(&lua).unwrap();
        assert_eq!(list.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(), ["Tierra Gorge", "KVM (Level 80 and up)"]);
        let kvm = &list[1];
        assert_eq!((kvm.sizes, kvm.applies, kvm.condition), ([5, 5], [true, true, false], (2, [80, 0])));
        assert_eq!((kvm.rewards[0].as_str(), kvm.goal.as_str(), kvm.picture.as_str()), ("5 Badges", "More kills", "bat_c01.bmp"));
        assert_eq!(list[0].condition, (3, [60, 79]));
    }

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

    #[test]
    fn reads_a_styling_shop_table() {
        let source = r#"
            StylingShop = {
                HeadPalette = { [1] = { goods = 1, payment = { zeny = 100000 }, isDoram = true } },
                HeadStyle = {
                    [2] = { goods = 2, payment = { zeny = 100000 }, isColoring = false },
                    [24] = { goods = 24, payment = { itid = 7622, boxitid = 16843 } }
                },
                DoramHeadStyle = { [2] = { goods = 2, payment = { zeny = 100000 }, isColoring = false } },
                BodyPalette = {
                    [1] = { goods = 0, payment = { itid = 6047, boxitid = 16855 } },
                    [2] = { goods = 2, payment = { itid = 6046, boxitid = 16854 } }
                },
                DoramBodyPalette = { [1] = { goods = 0, payment = { itid = 6047, boxitid = 16855 } } },
                TopAccessory = { [1] = { goods = 2220, payment = { zeny = 1000 }, isDoram = true } },
                MidAccessory = {},
                BottomAccessory = { [1] = { goods = 2241, payment = { zeny = 5000 } } },
                BodyStyle = { [1] = { goods = 0, payment = { itid = 6959, boxitid = 6959 } } },
                CostumeChange = { [1] = { goods = 99, payment = { zeny = 1 } } }
            }
        "#;
        let shop = super::styling_shop(&[source.as_bytes().to_vec()]).unwrap();
        assert_eq!(shop.head_palette[&1], super::StyleRow { goods: 1, zeny: 100_000, is_doram: Some(true), ..super::StyleRow::default() });
        assert_eq!(shop.head_style[&2].is_coloring, Some(false));
        assert_eq!((shop.head_style[&24].item, shop.head_style[&24].box_item), (7622, 16843));
        assert_eq!(shop.body_palette[&1].goods, 0);
        assert_eq!((shop.body_palette[&2].item, shop.body_palette[&2].box_item), (6046, 16854));
        assert_eq!(shop.top_accessory[&1].goods, 2220);
        assert_eq!(shop.body_style[&1].item, 6959);
        assert!(shop.mid_accessory.is_empty());
        // Costume change is not one of the lists this window reads: goods 99 never lands in them.
        let goods = shop.body_style.values().chain(shop.head_style.values()).map(|row| row.goods);
        assert!(goods.into_iter().all(|goods| goods != 99));
    }

    #[test]
    fn runs_the_styling_shops_functions_then_its_lists() {
        let framework = r#"
            StylingShop = {}
            StylingShop.HeadPalette = {}
            StylingShop.BodyPalette = {}
            function StylingShop.AddHeadPalette(goods, payment, isDoram)
                StylingShop.HeadPalette[#StylingShop.HeadPalette + 1] = { goods = goods, payment = payment, isDoram = isDoram }
            end
            function StylingShop.AddBodyPalette(goods, payment)
                StylingShop.BodyPalette[#StylingShop.BodyPalette + 1] = { goods = goods, payment = payment }
            end
        "#;
        let lists = r#"
            StylingShop.AddHeadPalette(3, { zeny = 100000 }, true)
            StylingShop.AddBodyPalette(0, { itid = 6047, boxitid = 16855 })
        "#;
        let shop = super::styling_shop(&[framework.as_bytes().to_vec(), lists.as_bytes().to_vec()]).unwrap();
        assert_eq!(shop.head_palette[&1].goods, 3);
        assert_eq!((shop.body_palette[&1].goods, shop.body_palette[&1].item, shop.body_palette[&1].box_item), (0, 6047, 16855));
    }

    #[test]
    fn reads_reform_names_as_raw_bytes() {
        // "\196\247" is the bytes C4 F7 (Lua's \ddd escapes are decimal), not UTF-8. One name is stored as id to name.
        let names = super::item_db_names(
            br#"
            ItemDBNameTbl = {
                ["Box"] = 100, ["Sword"] = 200, ["Shard"] = 300,
                [400] = "\196\247"
            }
            "#
            .to_vec(),
        )
        .unwrap();
        assert_eq!(names.get(b"Box".as_slice()), Some(&100));
        assert_eq!(names.get(&vec![0xC4, 0xF7]), Some(&400));
        let reforms = super::reform_table(
            br#"
            ReformItemList = { ["Box"] = { 1 }, ["\196\247"] = { 2 }, ["Missing"] = { 3 } }
            ReformInfo = {
                [1] = {
                    BaseItem = "Sword", ResultItem = "\196\247",
                    Material = { Shard = 100, Box = 1 },
                    NeedRefineMin = 9, NeedRefineMax = 20, NeedOptionNumMin = 1,
                    IsEmptySocket = false, PreserveSocketItem = true, PreserveGrade = true,
                    ChangeRefineValue = -5, RandomOptionCode = "AD",
                    InformationString = { "leave this out" }
                },
                [2] = {
                    BaseItem = "Sword", ResultItem = "Box",
                    NeedRefineMin = 0, NeedRefineMax = 10,
                    IsEmptySocket = true, PreserveSocketItem = false, PreserveGrade = false
                }
            }
            "#
            .to_vec(),
            &names,
        )
        .unwrap();
        let box_entry = &reforms[&100][0];
        assert_eq!((box_entry.base, box_entry.result, box_entry.materials.as_slice()), (200, 400, &[(100, 1), (300, 100)][..]));
        assert_eq!((box_entry.refine_min, box_entry.refine_max, box_entry.need_options, box_entry.change_refine), (9, 20, 1, -5));
        assert_eq!((box_entry.empty_socket, box_entry.preserve_socket, box_entry.preserve_grade, box_entry.random_option.as_str()), (false, true, true, "AD"));
        let korean = &reforms[&400][0];
        assert_eq!((korean.base, korean.result, korean.empty_socket, korean.preserve_socket), (200, 100, true, false));
        assert!(!reforms.contains_key(&0) && reforms.len() == 2);
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

    #[test]
    fn reads_an_enchant_table_and_resolves_names_by_their_bytes() {
        let names = b"ItemDBNameTbl = { Sharp1 = 4818, Sharp2 = 4817, Mysterious = 25669, [\"\\128\\129\"] = 600028 }\n".to_vec();
        let list = b"
            function order(self, ...)
                self.SlotOrder = {}
                for i = 1, #arg do self.SlotOrder[i] = arg[i] end
            end
            local info = {
                TargetItemTbl = { \"\\128\\129\" },
                Condition = { MinRefine = 5, MinGrade = 1 },
                bApproveRandomOpt = false,
                Reset = { bReset = true, Rate = 100000, Zeny = 500000, MatTbl = {} },
                Slot = {
                    [3] = {
                        RequireTbl = { Zeny = 1000, MatTbl = { Mysterious = 2 } },
                        SuccessRate = 80000,
                        GradeBonusTbl = { [0] = 0, [1] = 5000 },
                        EnchantRateTbl = { [1] = { Sharp1 = 100000 } },
                        PerfectECTbl = { Sharp1 = { Zeny = 100000, MatTbl = { Mysterious = 20 } } },
                        UpgradeECTbl = { Sharp1 = { ResultItemDB = \"Sharp2\", Zeny = 200000, MatTbl = { Mysterious = 40 } } },
                    },
                },
            }
            order(info, 3, 2)
            Table = { [33] = info }
        "
        .to_vec();
        let tables = super::enchant_tables(&[list], names).unwrap();
        let table = &tables[&33];
        assert_eq!(table.targets, [600028]);
        assert_eq!(table.slot_order, [3, 2]);
        assert_eq!((table.min_refine, table.min_grade, table.approve_random_opt), (5, 1, false));
        let reset = table.reset.as_ref().unwrap();
        assert_eq!((reset.enabled, reset.rate, reset.cost.zeny), (true, 100000, 500000));
        let slot = table.slots[3].as_ref().unwrap();
        assert_eq!((slot.cost.zeny, slot.cost.materials.as_slice(), slot.success_rate, slot.bonus(1)), (1000, [(25669, 2)].as_slice(), 80000, 5000));
        assert_eq!(slot.rate_list(1), [(4818, 100000)]);
        assert!(slot.rate_list(0).is_empty());
        assert_eq!(slot.perfect[0].item, 4818);
        assert_eq!((slot.perfect[0].cost.zeny, slot.perfect[0].cost.materials.as_slice()), (100000, [(25669, 20)].as_slice()));
        assert_eq!(slot.upgrades[0].1.result, 4817);
        assert_eq!(slot.upgrades[0].1.cost.materials, [(25669, 40)]);
    }
}
