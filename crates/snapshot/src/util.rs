use crate::types::{Snapshot, SnapshotManifest};
use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Error, Read};
use std::path::PathBuf;

use defs::DbError;
use flate2::{Compression, read::GzEncoder};
use std::{
    io::{BufWriter, Write},
    path::Path,
};
use tar::Builder;
use uuid::Uuid;

#[inline]
fn metadata_file_name(id: &Uuid) -> String {
    format!("{}-index-meta.bin", id)
}

#[inline]
fn topology_file_name(id: &Uuid) -> String {
    format!("{}-index-topo.bin", id)
}

// source: https://stackoverflow.com/questions/69787906/how-to-hash-a-binary-file-in-rust
pub fn sha256_digest(path: &PathBuf) -> Result<String, Error> {
    let input = File::open(path)?;
    let mut reader = BufReader::new(input);

    let digest = {
        let mut hasher = Sha256::new();
        let mut buffer = [0; 1024];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        hasher.finalize()
    };
    Ok(HEXLOWER.encode(digest.as_ref()))
}

impl Snapshot {
    pub fn save_metadata(
        path: &Path,
        uuid: Uuid,
        bytes: &[u8],
        magic: &[u8; 4],
    ) -> Result<PathBuf, DbError> {
        let file_name = metadata_file_name(&uuid);
        let metadata_file_path = path.join(file_name);

        let mut file = std::fs::File::create(metadata_file_path.clone()).map_err(|e| {
            DbError::SnapshotError(format!("Could not create metadata file: {}", e))
        })?;

        file.write_all(magic)
            .map_err(|e| DbError::SnapshotError(format!("Could not write metadata file: {}", e)))?;
        file.write_all(&bytes.len().to_le_bytes())
            .map_err(|e| DbError::SnapshotError(format!("Could not write metadata file: {}", e)))?;
        file.write_all(bytes)
            .map_err(|e| DbError::SnapshotError(format!("Could not write metadata file: {}", e)))?;

        Ok(metadata_file_path)
    }

    pub fn save_topology(
        path: &Path,
        uuid: Uuid,
        bytes: &[u8],
        magic: &[u8; 4],
    ) -> Result<PathBuf, DbError> {
        let file_name = topology_file_name(&uuid);
        let topology_file_path = path.join(file_name);

        let mut file = std::fs::File::create(topology_file_path.clone()).map_err(|e| {
            DbError::SnapshotError(format!("Could not create topology file: {}", e))
        })?;

        file.write_all(magic)
            .map_err(|e| DbError::SnapshotError(format!("Could not write topology file: {}", e)))?;
        file.write_all(&bytes.len().to_le_bytes())
            .map_err(|e| DbError::SnapshotError(format!("Could not write topology file: {}", e)))?;
        file.write_all(bytes)
            .map_err(|e| DbError::SnapshotError(format!("Could not write topology file: {}", e)))?;

        Ok(topology_file_path)
    }

    pub fn save_manifest(path: &Path, manifest: &SnapshotManifest) -> Result<PathBuf, Error> {
        let manifest_path = path.join("manifest.json");

        let file = std::fs::File::create(manifest_path.clone())?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, manifest)?;
        writer.flush()?;

        Ok(manifest_path)
    }

    pub fn compress_archive(path: &Path, files: &[&Path], base_dir: &Path) -> Result<(), Error> {
        let tar_gz = File::create(path)?;
        let enc = GzEncoder::new(tar_gz, Compression::default());
        let mut tar = Builder::new(enc);

        for file in files {
            let rel_path = file.strip_prefix(base_dir).unwrap_or(file);
            let mut f = File::open(file)?;
            tar.append_file(rel_path, &mut f)?;
        }

        tar.into_inner()?;
        Ok(())
    }
}
