//! Browser and computer-use tool registrations.
//!
//! Preconditions: browser tools use the agent_browser backend (CDP / CamoFox);
//! computer_use availability is determined by `check_computer_use_requirements`.

use std::sync::Arc;

use hermes_core::ToolHandler;

use super::{RegistryContext, reg};

pub fn register(ctx: &RegistryContext<'_>) {
    let browser_backend: Arc<dyn crate::tools::browser::BrowserBackend> =
        crate::backends::agent_browser::create_browser_backend();

    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserNavigateHandler::new(
            browser_backend.clone(),
        )),
        "🌐",
        vec![],
    );
    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserSnapshotHandler::new(
            browser_backend.clone(),
        )),
        "📸",
        vec![],
    );
    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserClickHandler::new(
            browser_backend.clone(),
        )),
        "🖱️",
        vec![],
    );
    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserTypeHandler::new(
            browser_backend.clone(),
        )),
        "⌨️",
        vec![],
    );
    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserScrollHandler::new(
            browser_backend.clone(),
        )),
        "📜",
        vec![],
    );
    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserBackHandler::new(
            browser_backend.clone(),
        )),
        "⬅️",
        vec![],
    );
    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserPressHandler::new(
            browser_backend.clone(),
        )),
        "🔘",
        vec![],
    );
    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserGetImagesHandler::new(
            browser_backend.clone(),
        )),
        "🖼️",
        vec![],
    );
    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserVisionHandler::new(
            browser_backend.clone(),
        )),
        "👁️",
        vec![],
    );
    reg(
        ctx,
        "browser",
        Arc::new(crate::tools::browser::BrowserConsoleHandler::new(
            browser_backend,
        )),
        "🔧",
        vec![],
    );

    let handler = Arc::new(crate::tools::computer_use::ComputerUseHandler::with_default_backend());
    let schema = handler.schema();
    let name = schema.name.clone();
    let desc = schema.description.clone();
    ctx.registry.register(
        name,
        "computer_use",
        schema,
        handler,
        Arc::new(crate::tools::computer_use::check_computer_use_requirements),
        vec![],
        true,
        desc,
        "🖱️",
        None,
    );
}
