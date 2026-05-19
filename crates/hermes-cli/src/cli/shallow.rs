//! Lightweight first-pass subcommand names (no per-command fields).

use clap::Subcommand;

/// First-pass subcommand routing table. Fields are parsed in a second pass.
#[derive(Debug, Clone, Subcommand)]
pub enum ShallowCommand {
    #[command(name = "hermes")]
    Hermes,
    Model    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Tools    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Config    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Gateway    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Doctor    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Update    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    #[command(name = "elite-check")]
    EliteCheck    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    #[command(name = "verify-provenance")]
    VerifyProvenance    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    #[command(name = "rotate-provenance-key")]
    RotateProvenanceKey    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    #[command(name = "route-learning")]
    RouteLearning    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    #[command(name = "route-health")]
    RouteHealth    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    #[command(name = "route-autotune")]
    RouteAutotune    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    #[command(name = "incident-pack")]
    IncidentPack    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Dashboard    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Debug    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Logs    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Profile    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Auth    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Secrets    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Cron    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Webhook    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Chat    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Skills    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Plugins    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Memory    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Mcp    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Sessions    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Resume    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Insights    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Login    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Logout    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Whatsapp    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Pairing    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Claw    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Acp    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Backup    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Import    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Dump    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Completion    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Uninstall    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Lumio    {
        #[arg(trailing_var_arg = true, hide = true, allow_hyphen_values = true)]
        _rest: Vec<String>,
    },
    Setup,
    Status,
    Version,
    #[command(external_subcommand)]
    PluginExternal(Vec<String>),
}
