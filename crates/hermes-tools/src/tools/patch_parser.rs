//! V4A patch parser compatibility exports.
//!
//! Corresponds to `hermes-agent/tools/patch_parser.py`.

pub use crate::v4a_patch::{
    Hunk, HunkLine, OperationType, PatchOperation, PatchResult, hunk_to_search_replace,
    parse_v4a_patch,
};
