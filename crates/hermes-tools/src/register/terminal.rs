//! Terminal and process tool registrations.
//!
//! Preconditions: all three tools require `TERMINAL_ENV` to resolve to a
//! reachable backend (checked via `terminal_check`).

use std::sync::Arc;

use super::{RegistryContext, reg_with_check};

pub fn register(ctx: &RegistryContext<'_>) {
    reg_with_check(
        ctx,
        "terminal",
        Arc::new(crate::tools::terminal::TerminalHandler::new(
            ctx.terminal_backend.clone(),
        )),
        "💻",
        vec![],
        ctx.terminal_check.clone(),
    );
    reg_with_check(
        ctx,
        "terminal",
        Arc::new(crate::tools::terminal::ProcessHandler::new(Arc::new(
            crate::tools::terminal::TerminalProcessBackendAdapter::new(
                ctx.terminal_backend.clone(),
            ),
        ))),
        "🧵",
        vec![],
        ctx.terminal_check.clone(),
    );
    reg_with_check(
        ctx,
        "terminal",
        Arc::new(crate::tools::process_registry::ProcessRegistryHandler::default()),
        "📊",
        vec![],
        ctx.terminal_check.clone(),
    );
}
