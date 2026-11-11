//! Keeping the map rotation in step with a folder of `.ogz` files.

use super::{LoadedMap, PlayMaps};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::fs;
use tracing::{error, info, warn};

/// A folder of maps and when each file was last loaded from it.
pub struct MapDir {
    dir: PathBuf,
    maps: Arc<RwLock<PlayMaps>>,
    loaded: HashMap<String, SystemTime>,
}

impl MapDir {
    /// Loads every map in `dir` into the rotation.
    pub async fn open(dir: PathBuf, maps: Arc<RwLock<PlayMaps>>) -> Self {
        let mut this = Self {
            dir,
            maps,
            loaded: HashMap::new(),
        };
        this.scan().await;
        this
    }

    /// Follows the folder from now on: new and changed files get (re)loaded,
    /// deleted ones leave the rotation. Runs until the program ends.
    pub async fn watch(mut self) {
        use notify::{RecursiveMode, Watcher};
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok() {
                let _ = tx.send(());
            }
        });
        // Without events the folder is still checked every few seconds.
        let _watcher = match watcher {
            Ok(mut w) => match w.watch(&self.dir, RecursiveMode::NonRecursive) {
                Ok(()) => Some(w),
                Err(e) => {
                    warn!("Can't watch map dir {:?}: {e}", self.dir);
                    None
                }
            },
            Err(e) => {
                warn!("Can't watch map dir {:?}: {e}", self.dir);
                None
            }
        };

        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                Some(()) = rx.recv() => {
                    // Let a burst of events (a git pull, a cloud sync) settle.
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    while rx.try_recv().is_ok() {}
                }
            }
            self.scan().await;
        }
    }

    async fn scan(&mut self) {
        let Ok(mut entries) = fs::read_dir(&self.dir).await else {
            warn!("Failed to read map directory {:?}", self.dir);
            return;
        };
        let mut present = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let Some(name) = ogz_name(&path) else {
                continue;
            };
            present.push(name.clone());
            let Ok(modified) = entry.metadata().await.and_then(|m| m.modified()) else {
                continue;
            };
            if self.loaded.get(&name).is_some_and(|&t| t >= modified) {
                continue;
            }
            match load(&path, name.clone()).await {
                Ok(map) => {
                    self.maps.write().insert(name.clone(), map);
                    info!("Map dir: loaded '{name}'");
                    self.loaded.insert(name, modified);
                }
                Err(e) => {
                    error!("Map dir: failed to load '{name}': {e}");
                    // Tried again once the file changes, not on every scan.
                    self.loaded.insert(name, modified);
                }
            }
        }
        let gone: Vec<String> = self
            .loaded
            .keys()
            .filter(|name| !present.contains(name))
            .cloned()
            .collect();
        for name in gone {
            self.maps.write().remove_map(&name);
            self.loaded.remove(&name);
            info!("Map dir: removed '{name}'");
        }
    }
}

/// The map name of a `.ogz` file.
fn ogz_name(path: &Path) -> Option<String> {
    if path.extension()? != "ogz" {
        return None;
    }
    Some(path.file_stem()?.to_str()?.to_string())
}

/// Reads and parses a map file on a blocking thread, away from the lock.
async fn load(path: &Path, name: String) -> Result<LoadedMap, String> {
    let data = fs::read(path).await.map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || LoadedMap::parse(&name, &data))
        .await
        .map_err(|e| e.to_string())?
}
