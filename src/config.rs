use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[derive(Debug, Clone)]
pub enum LoadError {
    FileAccess,
    SerdeFmt,
    WriteDefault,
}

#[derive(Debug, Clone)]
pub enum SaveError {
    FileAccess,
    Write,
    SerdeFmt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub discord_key: String,
}

impl Config {
    fn path() -> std::path::PathBuf {
        let mut path = if let Some(project_dirs) = directories_next::BaseDirs::new() {
            project_dirs.config_dir().into()
        } else {
            std::env::current_dir().unwrap_or_default()
        };
        path.push(format!("{}/config.json", env!("CARGO_PKG_NAME")));
        path
    }

    pub async fn load() -> Result<Config, LoadError> {
        let mut contents = String::new();
        let file = tokio::fs::File::open(Self::path())
            .await
            .map_err(|_| LoadError::FileAccess);
        match file {
            Ok(mut f) => {
                f.read_to_string(&mut contents)
                    .await
                    .map_err(|_| LoadError::FileAccess)?;
                serde_json::from_str(&contents).map_err(|_| LoadError::SerdeFmt)
            }
            _ => {
                // config file does not exist, auto-create it
                Config::save(Config {
                    discord_key: "".into(),
                })
                .await
                .map_err(|_| LoadError::WriteDefault)
            }
        }
    }

    pub async fn save(self) -> Result<Self, SaveError> {
        let json = serde_json::to_string_pretty(&self).map_err(|_| SaveError::SerdeFmt)?;
        let path = Self::path();
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|_| SaveError::FileAccess)?;
        }
        {
            let mut file = tokio::fs::File::create(path)
                .await
                .map_err(|_| SaveError::FileAccess)?;

            file.write_all(json.as_bytes())
                .await
                .map_err(|_| SaveError::Write)?;
        }
        Ok(self)
    }
}
