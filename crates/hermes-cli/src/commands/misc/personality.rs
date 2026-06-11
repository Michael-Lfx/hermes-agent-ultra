//! `/personality` slash command handler.

use crate::commands::{CommandResult, emit_command_output, format_personality_catalog};
use hermes_core::AgentError;

pub(crate) fn handle_personality_command(
    host: &mut impl crate::app::SlashCommandHost,
    args: &[&str],
) -> Result<CommandResult, AgentError> {
    let builtin = hermes_agent::builtin_personality_names();
    let builtin_descriptions = hermes_agent::builtin_personality_descriptions();
    if args.is_empty() {
        emit_command_output(
            host,
            format_personality_catalog(host.current_personality(), builtin_descriptions),
        );
    } else if args.len() == 1 && args[0].eq_ignore_ascii_case("list") {
        emit_command_output(
            host,
            format_personality_catalog(host.current_personality(), builtin_descriptions),
        );
    } else {
        let name = args.join(" ");
        host.switch_personality(&name);
        let mut response = format!("Switched personality to `{}`.", name);
        if !name.contains(char::is_whitespace)
            && !name.eq_ignore_ascii_case("default")
            && !builtin.iter().any(|n| n.eq_ignore_ascii_case(&name))
        {
            response.push_str(&format!(
                "\n\nNote: `{}` is not built-in. Hermes will look for `personalities/{}.md` or treat inline text as compatibility mode.",
                name, name,
            ));
        }
        emit_command_output(host, response);
    }
    Ok(CommandResult::Handled)
}
