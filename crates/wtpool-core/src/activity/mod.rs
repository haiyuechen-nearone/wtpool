//! The activity records in `<git-common-dir>/wtpool-activity/` that `wtpool hook` writes and
//! `wtpool status` reads: their folder and files, the [`ActorKey`] each file is named after, the
//! ids an adapter checks and the values a record holds.

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
