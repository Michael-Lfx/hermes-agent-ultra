use crate::{
    state::AppState,
    ws::rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
};

pub mod attachment;
pub mod completion;
pub mod config;
pub mod file_resolver;
pub mod handoff;
pub mod interaction;
pub mod model;
pub mod prompt;
pub mod session;
pub mod slash;
pub mod tools;

/// Dispatch a JSON-RPC request to the appropriate handler.
pub async fn dispatch(
    request: JsonRpcRequest,
    state: &AppState,
) -> Option<JsonRpcResponse> {
    // Validate request
    if let Err(err) = request.validate() {
        return Some(JsonRpcResponse::err(request.id, err));
    }
    
    match request.method.as_str() {
        // Session management
        "session.create" => session::handle_create(request, state).await,
        "session.list" => session::handle_list(request, state).await,
        "session.resume" => session::handle_resume(request, state).await,
        "session.close" => session::handle_close(request, state).await,
        "session.history" => session::handle_history(request, state).await,
        "session.interrupt" => session::handle_interrupt(request, state).await,
        "session.title" => session::handle_title(request, state).await,
        "session.usage" => session::handle_usage(request, state).await,
        "session.delete" => session::handle_delete(request, state).await,
        "session.steer" => prompt::handle_steer(request, state).await,
        
        // Prompt
        "prompt.submit" => prompt::handle_submit(request, state).await,
        "prompt.background" => prompt::handle_background(request, state).await,
        
        // Config
        "config.get" => config::handle_config_get(request, state).await,
        "config.set" => config::handle_config_set(request, state).await,
        "config.show" => config::handle_config_show(request, state).await,
        
        // Model
        "model.options" => model::handle_model_options(request, state).await,
        "model.save_key" => model::handle_model_save_key(request, state).await,
        "model.disconnect" => model::handle_model_disconnect(request, state).await,
        
        // Tools / Skills
        "tools.list" => tools::handle_tools_list(request, state).await,
        "tools.show" => tools::handle_tools_show(request, state).await,
        "tools.configure" => tools::handle_tools_configure(request, state).await,
        "skills.manage" => tools::handle_skills_manage(request, state).await,
        "skills.reload" => tools::handle_skills_reload(request, state).await,
        
        // Slash commands
        "slash.exec" => slash::handle_slash_exec(request, state).await,
        "command.dispatch" => slash::handle_slash_exec(request, state).await, // Alias for compatibility
        
        // Reload
        "reload.mcp" => tools::handle_reload_mcp(request, state).await,
        "reload.env" => tools::handle_reload_env(request, state).await,
        
        // Completion
        "complete.path" => completion::handle_complete_path(request, state).await,
        "complete.slash" => completion::handle_complete_slash(request, state).await,
        
        // Attachments
        "image.attach" => attachment::handle_image_attach(request, state).await,
        "image.attach_bytes" => attachment::handle_image_attach_bytes(request, state).await,
        "file.attach" => attachment::handle_file_attach(request, state).await,
        
        // Handoff
        "handoff.request" => handoff::handle_handoff_request(request, state).await,
        "handoff.state" => handoff::handle_handoff_state(request, state).await,
        "handoff.fail" => handoff::handle_handoff_fail(request, state).await,
        
        // Interaction responses
        "approval.respond" => interaction::handle_approval_respond(request, &state.pending_interactions).await,
        "clarify.respond" => interaction::handle_clarify_respond(request, &state.pending_interactions).await,
        "sudo.respond" => interaction::handle_sudo_respond(request, &state.pending_interactions).await,
        "secret.respond" => interaction::handle_secret_respond(request, &state.pending_interactions).await,
        
        // Unknown method
        _ => Some(JsonRpcResponse::err(
            request.id,
            JsonRpcError::method_not_found(format!("method '{}' not found", request.method)),
        )),
    }
}
