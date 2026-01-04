// what do i need this to do?
// manage a source of snapshots and load and unload metadata
// load latest snapshot, load latest snapshot before x (ok but)
// manage a worker thread that is woken up at regular intervals or at the call of a function to take snapshot
// it accepts an arc
//
// broad architecture - what i want:
// - abstract snapshot source -> can be local directory or remote(define protocol)
// source operations:
//      - add snapshot (with Snapshot)
//      - read snapshot metadatas with paging
//      - read Snapshot of specific snapshot - internal implementation: unpack and read manifest file`(dont bother with checksums verification)
//             - make a proxy wrapper that deletes the temp file on destroy - caching is internal implementation
//
