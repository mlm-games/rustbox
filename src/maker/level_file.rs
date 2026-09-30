use rustbox_format::file::{FORMAT_VERSION, LevelFile, LevelFileV3, upgrade_v3};
use rustbox_format::level::LevelData;

pub fn serialize_level(level: &LevelData) -> anyhow::Result<String> {
    let file = LevelFile {
        version: FORMAT_VERSION,
        level: level.clone(),
    };
    Ok(ron::ser::to_string_pretty(
        &file,
        ron::ser::PrettyConfig::default(),
    )?)
}

pub fn deserialize_level(text: &str) -> anyhow::Result<LevelData> {
    #[derive(serde::Deserialize)]
    struct LevelHeader {
        version: u32,
    }
    let header: LevelHeader =
        ron::from_str(text).map_err(|_| anyhow::anyhow!("could not read level file"))?;
    let data = match header.version {
        4 | 5 | 6 | 7 | 8 => {
            let file: LevelFile =
                ron::from_str(text).map_err(|_| anyhow::anyhow!("corrupted level file"))?;
            file.level
        }
        1 | 2 | 3 => {
            let file: LevelFileV3 =
                ron::from_str(text).map_err(|_| anyhow::anyhow!("corrupted level file"))?;
            upgrade_v3(file.level)
        }
        v => anyhow::bail!("unknown level format version {v}"),
    };
    rustbox_format::file::validate_level(&data)
        .map_err(|e| anyhow::anyhow!("invalid level: {e}"))?;
    Ok(data)
}
