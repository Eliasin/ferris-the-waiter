use crate::CommandId;
use leptos::*;

#[cfg(feature = "ssr")]
use {
    crate::{
        check_session_token, CommandOutputs, Commands, PasswordHashString, RateLimiting,
        SessionTokens,
    },
    axum::extract::Extension,
    leptos_axum::extract,
    std::process::Stdio,
    std::sync::{Arc, Mutex},
};

#[server(Login, "/ferris/api")]
pub async fn login(password: String) -> Result<String, ServerFnError> {
    let (Extension(rate_limiting), Extension(password_hash), Extension(session_tokens)): (
        Extension<Arc<Mutex<RateLimiting>>>,
        Extension<Arc<PasswordHashString>>,
        Extension<Arc<Mutex<SessionTokens>>>,
    ) = extract().await?;

    {
        let rate_limiting = rate_limiting.lock().unwrap();

        let now = std::time::Instant::now();

        if let Some(last_request_time) = (*rate_limiting).last_request_time {
            if now.duration_since(last_request_time) < std::time::Duration::from_millis(500) {
                return Err(ServerFnError::ServerError("Rate limited".to_string()));
            }
        }
    }

    let bcrypt_result = tokio::task::spawn_blocking(move || {
        bcrypt::verify(password.as_bytes(), password_hash.0.as_str())
    })
    .await?;

    if !bcrypt_result? {
        {
            let mut rate_limiting = rate_limiting.lock().unwrap();

            let now = std::time::Instant::now();
            rate_limiting.last_request_time = Some(now);
        }
        return Err(ServerFnError::ServerError("Invalid password".to_string()));
    }

    let session_token = rand::random::<u64>().to_string();

    session_tokens
        .lock()
        .unwrap()
        .0
        .insert(session_token.clone(), chrono::Utc::now());

    Ok(session_token)
}

#[server(CommandList, "/ferris/api")]
pub async fn command_list(session_token: String) -> Result<Vec<String>, ServerFnError> {
    let (Extension(commands), Extension(session_tokens)): (
        Extension<Arc<Commands>>,
        Extension<Arc<Mutex<SessionTokens>>>,
    ) = extract().await?;

    check_session_token(&mut session_tokens.lock().unwrap(), session_token.as_str())?;
    Ok(commands.0.keys().cloned().collect())
}

#[server(InvokeCommand, "/ferris/api")]
pub async fn invoke_command(
    command_name: String,
    session_token: String,
) -> Result<CommandId, ServerFnError> {
    use tokio::process::Command;

    let (Extension(commands), Extension(session_tokens), Extension(command_outputs)): (
        Extension<Arc<Commands>>,
        Extension<Arc<Mutex<SessionTokens>>>,
        Extension<Arc<Mutex<CommandOutputs>>>,
    ) = extract().await?;

    check_session_token(&mut session_tokens.lock().unwrap(), session_token.as_str())?;

    match commands.0.get(&command_name) {
        None => Err(ServerFnError::ServerError(
            "No command with that name".to_string(),
        )),
        Some(command_str) => {
            let command_id = CommandId(rand::random::<u32>());
            let mut command = Command::new("sh");
            command
                .env("TERM", "xterm")
                .arg("-c")
                .args(shlex::Shlex::new(format!("\"{command_str}\"").as_str()))
                .stdout(Stdio::piped())
                .stdin(Stdio::null())
                .stderr(Stdio::piped());

            let child = command.spawn()?;

            let mut command_outputs = command_outputs.lock().unwrap();
            command_outputs
                .0
                .insert(command_id, (command_name.clone(), child));

            Ok(command_id)
        }
    }
}

#[server(CommandExecutionList, "/ferris/api")]
pub async fn command_execution_list(
    session_token: String,
) -> Result<Vec<CommandId>, ServerFnError> {
    let (Extension(session_tokens), Extension(command_outputs)): (
        Extension<Arc<Mutex<SessionTokens>>>,
        Extension<Arc<Mutex<CommandOutputs>>>,
    ) = extract().await?;

    check_session_token(&mut session_tokens.lock().unwrap(), session_token.as_str())?;

    let command_outputs = command_outputs.lock().unwrap();
    Ok(command_outputs.0.keys().cloned().collect())
}
