//! File-system tool registrations.
//!
//! Preconditions: read_file and write_file require `TERMINAL_ENV` to resolve
//! to a reachable backend; patch and search_files run locally with no env dep.

use std::sync::Arc;

use super::{RegistryContext, reg, reg_with_check};

pub fn register(ctx: &RegistryContext<'_>) {
    reg_with_check(
        ctx,
        "file",
        Arc::new(crate::tools::file::ReadFileHandler::new(
            ctx.terminal_backend.clone(),
        )),
        "📖",
        vec![],
        ctx.terminal_check.clone(),
    );
    reg_with_check(
        ctx,
        "file",
        Arc::new(crate::tools::file::WriteFileHandler::new(
            ctx.terminal_backend.clone(),
        )),
        "✏️",
        vec![],
        ctx.terminal_check.clone(),
    );
    reg(
        ctx,
        "file",
        Arc::new(crate::tools::file::PatchHandler::new(Arc::new(
            crate::backends::file::LocalPatchBackend::new(),
        ))),
        "🩹",
        vec![],
    );
    reg(
        ctx,
        "file",
        Arc::new(crate::tools::file::SearchFilesHandler::new(Arc::new(
            crate::backends::file::LocalSearchBackend::new(),
        ))),
        "🔎",
        vec![],
    );
}
