use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Error, Read};
use std::path::PathBuf;

use defs::{DbError, Magic};
use flate2::{Compression, write::GzEncoder};
use std::{io::Write, path::Path};
use tar::Builder;
use uuid::Uuid;

type BinFileContent = (Magic, Vec<u8>);

#[inline]
pub fn metadata_filename(id: &Uuid) -> String {
    format!("{}-index-meta.bin", id)
}

#[inline]
pub fn topology_filename(id: &Uuid) -> String {
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

pub fn save_index_metadata(
    path: &Path,
    uuid: Uuid,
    bytes: &[u8],
    magic: &Magic,
) -> Result<PathBuf, DbError> {
    let file_name = metadata_filename(&uuid);
    let metadata_file_path = path.join(file_name);

    let mut file = std::fs::File::create(metadata_file_path.clone())
        .map_err(|e| DbError::SnapshotError(format!("Could not create metadata file: {}", e)))?;

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
    magic: &Magic,
) -> Result<PathBuf, DbError> {
    let file_name = topology_filename(&uuid);
    let topology_file_path = path.join(file_name);

    let mut file = std::fs::File::create(topology_file_path.clone())
        .map_err(|e| DbError::SnapshotError(format!("Could not create topology file: {}", e)))?;

    file.write_all(magic)
        .map_err(|e| DbError::SnapshotError(format!("Could not write topology file: {}", e)))?;
    file.write_all(&bytes.len().to_le_bytes())
        .map_err(|e| DbError::SnapshotError(format!("Could not write topology file: {}", e)))?;
    file.write_all(bytes)
        .map_err(|e| DbError::SnapshotError(format!("Could not write topology file: {}", e)))?;

    Ok(topology_file_path)
}

pub fn compress_archive(path: &Path, files: &[&Path]) -> Result<(), Error> {
    let tar_gz = File::create(path)?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = Builder::new(enc);

    for file in files {
        let rel_path = file.file_name().unwrap();
        let mut f = File::open(file)?;
        tar.append_file(rel_path, &mut f)?;
    }

    tar.into_inner()?;
    Ok(())
}

pub fn read_index_topology(path: &Path) -> Result<BinFileContent, DbError> {
    let mut file = File::open(path)
        .map_err(|e| DbError::SnapshotError(format!("Couldn't open topology file: {}", e)))?;

    let mut magic = Magic::default();
    file.read_exact(&mut magic).map_err(|e| {
        DbError::SnapshotError(format!("Couldn't read magic from topology file: {}", e))
    })?;

    let mut len_bytes = [0u8; size_of::<usize>()];
    file.read_exact(&mut len_bytes).map_err(|e| {
        DbError::SnapshotError(format!("Couldn't read length from topology file: {}", e))
    })?;
    let len = usize::from_le_bytes(len_bytes);

    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes).map_err(|e| {
        DbError::SnapshotError(format!("Couldn't read bytes from topology file: {}", e))
    })?;

    Ok((magic, bytes))
}

pub fn read_index_metadata(path: &Path) -> Result<BinFileContent, DbError> {
    let mut file = File::open(path)
        .map_err(|e| DbError::SnapshotError(format!("Couldn't open metadata file: {}", e)))?;

    let mut magic = Magic::default();
    file.read_exact(&mut magic).map_err(|e| {
        DbError::SnapshotError(format!("Couldn't read magic from metadata file: {}", e))
    })?;

    let mut len_bytes = [0u8; size_of::<usize>()];
    file.read_exact(&mut len_bytes).map_err(|e| {
        DbError::SnapshotError(format!("Couldn't read length from metadata file: {}", e))
    })?;

    let len = usize::from_le_bytes(len_bytes);
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes).map_err(|e| {
        DbError::SnapshotError(format!("Couldn't read bytes from metadata file: {}", e))
    })?;

    Ok((magic, bytes))
}
