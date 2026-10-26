//! The activity records in `<git-common-dir>/wtpool-activity/` as `wtpool hook` writes them and
//! `wtpool status` reads them (status-panel.md, section 1): the folder and its record files, the
//! actor each file is named after (H17, H25), the ids an adapter checks (H17, H27) and the values
//! a record holds.

mod files;
mod harness;
mod ids;
mod key;
mod legacy;
mod values;

pub use files::{
    ACTIVITY_DIR, MAX_RECORD_BYTES, RECORD_VERSION, create_activity, read_record_file,
};
pub use harness::{Harness, UnknownHarness};
pub use ids::{ActorId, CallId, InvalidId, TaskId};
pub use key::{ActorKey, Stem};
pub use legacy::legacy_kind;
pub use values::{CallKind, PlacedBy, State, WaitReason};
