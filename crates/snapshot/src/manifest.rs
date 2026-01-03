use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{
    io::{BufReader, BufWriter, Error, Write},
    path::PathBuf,
};
use uuid::Uuid;

use crate::constants::MANIFEST_FILE;

type UnixTimestamp = i64;

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub id: Uuid,
    pub date: UnixTimestamp,
    pub sem_ver: String,
    pub index_metadata_checksum: String,
    pub index_topo_checksum: String,
    pub storage_checkpoint_checksum: String,
}

impl Manifest {
    pub fn save(&self, path: &Path) -> Result<PathBuf, Error> {
        let manifest_path = path.join(MANIFEST_FILE);

        let file = std::fs::File::create(manifest_path.clone())?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, self)?;
        writer.flush()?;

        Ok(manifest_path)
    }

    pub fn load(path: &Path) -> Result<Self, Error> {
        let file = std::fs::File::open(path)?;
        let mut reader = BufReader::new(file);
        let manifest: Manifest = serde_json::from_reader(&mut reader)?;
        Ok(manifest)
    }
}
