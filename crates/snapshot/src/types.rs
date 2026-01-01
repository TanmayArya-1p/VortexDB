use semver::Version;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::SystemTime};
use uuid::Uuid;

pub struct Snapshot {
    pub id: Uuid,
    pub date: SystemTime,
    pub path: PathBuf,
    pub sem_ver: Version,
}

type UnixTimestamp = i64;

#[derive(Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub id: Uuid,
    pub date: UnixTimestamp,
    pub sem_ver: String,
    pub index_metadata_checksum: String,
    pub index_topo_checksum: String,
    pub storage_checkpoint_checksum: String,
}

// TODO: NOTES
// manifest should have checksums of topo , metadata , storage, parser version, date created, snapshot metadata: id, date, sem_ver
// shoudl decode filename only when SnapshotEngine is being used otherwise it is pretty much useless
