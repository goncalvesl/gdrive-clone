//! Production mirror contracts. Planning remains pure; absence is classified by adapters.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    #[serde(default)]
    pub directory: bool,
    pub hash: String,
    pub size: u64,
    /// Local dev:inode or remote revision. Used for preconditions, not content equality.
    pub identity: String,
}
impl Entry {
    pub fn same_content(&self, other: &Self) -> bool {
        self.directory == other.directory && self.hash == other.hash && self.size == other.size
    }
    pub fn same_file(&self, other: &Self) -> bool {
        self.path == other.path && self.same_content(other)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub id: String,
    pub local: Entry,
    pub remote: Entry,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Upload,
    Download,
    TrashRemote,
    QuarantineLocal,
    Checkpoint,
    Forget,
    KeepBoth,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub operation_id: String,
    pub object_id: String,
    pub kind: Kind,
    pub local: Option<Entry>,
    pub remote: Option<Entry>,
    pub base: Option<Link>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    Act(Kind),
    Conflict,
    Unknown,
}

/// A missing remote object MUST have explicit trash evidence to propagate deletion.
/// Local absence requires a complete scan of the configured, unchanged root.
pub fn reconcile(
    base: Option<&Link>,
    local: Option<&Entry>,
    remote: Option<&Entry>,
    remote_trashed: bool,
) -> Choice {
    if local
        .zip(remote)
        .is_some_and(|(l, r)| l.directory != r.directory)
    {
        return Choice::Conflict;
    }
    match (base, local, remote) {
        (_, Some(l), Some(r)) if l.same_file(r) => Choice::Act(Kind::Checkpoint),
        (None, Some(_), None) if !remote_trashed => Choice::Act(Kind::Upload),
        (None, None, Some(_)) => Choice::Act(Kind::Download),
        (None, Some(_), Some(_)) => Choice::Conflict,
        (Some(b), Some(l), Some(r)) => {
            let lc = !l.same_file(&b.local);
            let rc = !r.same_file(&b.remote);
            match (lc, rc) {
                (true, false) => Choice::Act(Kind::Upload),
                (false, true) => Choice::Act(Kind::Download),
                (false, false) => Choice::Act(Kind::Checkpoint),
                (true, true) => Choice::Conflict,
            }
        }
        (Some(b), None, Some(r)) if r.same_file(&b.remote) => Choice::Act(Kind::TrashRemote),
        (Some(_), None, Some(_)) => Choice::Conflict,
        (Some(b), Some(l), None) if remote_trashed && l.same_file(&b.local) => {
            Choice::Act(Kind::QuarantineLocal)
        }
        (Some(_), Some(_), None) if remote_trashed => Choice::Conflict,
        (Some(_), None, None) if remote_trashed => Choice::Act(Kind::Forget),
        _ => Choice::Unknown,
    }
}

/// Private transfer state; the session URL must never enter user-facing reports.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadSession {
    pub url: String,
    pub acknowledged: u64,
    pub target: Entry,
    pub expected: Option<Entry>,
}
