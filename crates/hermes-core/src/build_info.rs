pub fn startup_commit_info() -> (&'static str, &'static str) {
    let commit = option_env!("HERMES_GIT_COMMIT")
        .or(option_env!("VERGEN_GIT_SHA"))
        .unwrap_or("unknown");
    (env!("CARGO_PKG_VERSION"), commit)
}
