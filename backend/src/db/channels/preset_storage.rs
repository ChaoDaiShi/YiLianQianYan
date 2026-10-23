use crate::{
    db::{
        migrations::{MigrationOwner, ProductMigrationSpec},
        Database,
    },
    modules::channel::ChannelPreset,
};
use rusqlite::{params, OptionalExtension};

const EXTENSIONS: ProductMigrationSpec = ProductMigrationSpec::new(
    1058, "1058_channel_presets_and_runtime", MigrationOwner::V1TaskWorld,
    "CREATE TABLE channel_presets(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
     CREATE TABLE channel_runtime_state(channel_id TEXT PRIMARY KEY REFERENCES external_channels(id),payload TEXT NOT NULL);",
);

impl Database {
    pub(super) fn initialize_channel_presets(&self) -> Result<(), rusqlite::Error> {
        self.apply_product_migration(&EXTENSIONS)?;
        for preset in crate::modules::channel::presets::builtin_presets() {
            self.conn().execute(
                "INSERT OR IGNORE INTO channel_presets(id,payload) VALUES(?1,?2)",
                params![
                    preset.id,
                    serde_json::to_string(&preset).expect("preset serializes")
                ],
            )?;
        }
        Ok(())
    }

    pub fn channel_presets(&self) -> Result<Vec<ChannelPreset>, String> {
        let conn = self.conn();
        let mut statement = conn.prepare("SELECT payload FROM channel_presets ORDER BY json_extract(payload,'$.builtin') DESC,json_extract(payload,'$.created_at'),id").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }

    pub fn channel_preset(&self, id: &str) -> Result<Option<ChannelPreset>, String> {
        let raw: Option<String> = self
            .conn()
            .query_row(
                "SELECT payload FROM channel_presets WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        raw.map(|value| serde_json::from_str(&value).map_err(|e| e.to_string()))
            .transpose()
    }

    pub fn save_channel_preset(&self, preset: &ChannelPreset) -> Result<(), String> {
        if preset.builtin || self.channel_preset(&preset.id)?.is_some_and(|p| p.builtin) {
            return Err("内置预设只读，请复制为自定义预设".into());
        }
        self.conn().execute("INSERT INTO channel_presets(id,payload) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![preset.id,serde_json::to_string(preset).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }

    pub fn delete_channel_preset(&self, id: &str) -> Result<(), String> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let builtin: Option<bool> = tx
            .query_row(
                "SELECT json_extract(payload,'$.builtin') FROM channel_presets WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if builtin == Some(true) {
            return Err("内置预设不能删除".into());
        }
        let referenced: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM external_channels WHERE json_extract(payload,'$.preset_id')=?1)",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
        if referenced {
            return Err("此预设正在被渠道使用，请先为渠道改选预设".into());
        }
        tx.execute("DELETE FROM channel_presets WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    pub fn update_channel_preset(&self, preset: &ChannelPreset) -> Result<bool, String> {
        if preset.builtin {
            return Err("内置预设只读，请复制为自定义预设".into());
        }
        Ok(self.conn().execute("UPDATE channel_presets SET payload=?2 WHERE id=?1 AND json_extract(payload,'$.builtin')=0",params![preset.id,serde_json::to_string(preset).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())? > 0)
    }
}
