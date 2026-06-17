use hermes_core::AgentError;
use hermes_skills::{
    BundledLayout, reconcile_bundled_updates, set_bundled_skills_opt_out, sync_skills,
};

pub(crate) fn run_sync(quiet: bool) -> Result<(), AgentError> {
    let layout = BundledLayout::resolve();
    if !layout.bundled_exists() {
        return Err(AgentError::Config(
            "Bundled skills directory not found. Install a full release package or run from a repo checkout.".into(),
        ));
    }
    let config = layout.sync_config();
    let result = sync_skills(&config, quiet).map_err(|e| AgentError::Config(e.to_string()))?;
    if result.skipped_opt_out {
        println!("Skipped bundled skill sync (profile opted out via .no-bundled-skills).");
        return Ok(());
    }
    println!(
        "Bundled skills sync: copied={}, updated={}, skipped={}, user_modified={}, cleaned={}",
        result.copied.len(),
        result.updated.len(),
        result.skipped,
        result.user_modified.len(),
        result.cleaned.len()
    );
    if !result.copied.is_empty() {
        println!("  copied: {}", result.copied.join(", "));
    }
    if !result.updated.is_empty() {
        println!("  updated: {}", result.updated.join(", "));
    }
    if !result.user_modified.is_empty() {
        println!(
            "  user_modified (skipped): {}",
            result.user_modified.join(", ")
        );
    }
    if !result.errors.is_empty() {
        for err in &result.errors {
            eprintln!("  error: {err}");
        }
    }
    Ok(())
}

pub(crate) fn run_opt_out() -> Result<(), AgentError> {
    let home = hermes_config::hermes_home();
    let result =
        set_bundled_skills_opt_out(&home, true).map_err(|e| AgentError::Config(e.to_string()))?;
    println!("{}", result.message);
    Ok(())
}

pub(crate) fn run_opt_in(extra: Option<&str>) -> Result<(), AgentError> {
    let home = hermes_config::hermes_home();
    let result =
        set_bundled_skills_opt_out(&home, false).map_err(|e| AgentError::Config(e.to_string()))?;
    println!("{}", result.message);
    let do_sync = extra.is_some_and(|e| e.split_whitespace().any(|t| t == "--sync" || t == "sync"));
    if do_sync {
        run_sync(true)?;
    }
    Ok(())
}

pub(crate) fn run_reconcile_quiet() {
    let layout = BundledLayout::resolve();
    if !layout.bundled_exists() {
        tracing::debug!("post-update skills reconcile skipped: no bundled dir");
        return;
    }
    match reconcile_bundled_updates(&layout.sync_config(), true) {
        Ok(result) => tracing::info!(
            copied = result.copied.len(),
            updated = result.updated.len(),
            skipped_opt_out = result.skipped_opt_out,
            "bundled skills reconciled after update"
        ),
        Err(err) => tracing::warn!(error = %err, "bundled skills reconcile failed after update"),
    }
}
