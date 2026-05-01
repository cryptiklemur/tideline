use std::path::Path;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

pub struct PluginLog {
    file: Mutex<File>,
}

impl PluginLog {
    pub async fn open(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        Ok(Self { file: Mutex::new(file) })
    }

    pub async fn append_line(&self, level: &str, msg: &str) -> std::io::Result<()> {
        let line = format!(
            "{} {} {}\n",
            now_rfc3339(),
            level,
            msg
        );
        let mut f = self.file.lock().await;
        f.write_all(line.as_bytes()).await?;
        f.flush().await
    }
}

fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{}.{:09}", d.as_secs(), d.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_and_appends() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nested/plugin.log");
        let log = PluginLog::open(&p).await.unwrap();
        log.append_line("INFO", "hello").await.unwrap();
        log.append_line("WARN", "again").await.unwrap();
        let contents = tokio::fs::read_to_string(&p).await.unwrap();
        assert!(contents.contains("INFO hello"));
        assert!(contents.contains("WARN again"));
        assert_eq!(contents.lines().count(), 2);
    }
}
