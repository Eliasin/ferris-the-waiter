pub mod api;
pub mod app;
pub mod error_template;
#[cfg(feature = "ssr")]
pub mod fileserv;
#[cfg(feature = "ssr")]
use leptos::ServerFnErrorErr;
pub mod terminal;

use std::collections::HashMap;

use chrono::Utc;

#[derive(Default, serde::Serialize, serde::Deserialize, Debug)]
pub struct Commands(pub HashMap<String, String>);

#[derive(Clone, Debug)]
pub struct PasswordHashString(pub String);

#[derive(Debug)]
pub struct SessionTokens(pub HashMap<String, chrono::DateTime<Utc>>);

#[derive(Hash, PartialEq, Clone, Copy, Debug, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandId(pub u32);

#[cfg(feature = "ssr")]
pub struct CommandOutputs(pub HashMap<CommandId, (String, tokio::process::Child)>);

#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq)]
#[cfg_attr(feature = "ssr", derive(serde::Deserialize))]
pub enum ClientMessage {
    GetCommandOutputStream { command_id: u32 },
    UnsubscribeFromCommandOutput { command_id: u32 },
}

#[derive(Clone, serde::Deserialize)]
#[cfg_attr(feature = "ssr", derive(serde::Serialize))]
pub enum ServerMessage {
    NoCommandWithId {
        command_id: u32,
    },
    InvalidPassword,
    CommandOutput {
        command_id: CommandId,
        output: Vec<u8>,
    },
}

#[derive(Default, Clone)]
pub struct RateLimiting {
    pub last_request_time: Option<std::time::Instant>,
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::*;
    console_error_panic_hook::set_once();
    leptos::mount_to_body(App);
}

#[cfg(feature = "ssr")]
pub fn check_session_token(
    session_tokens: &mut SessionTokens,
    session_token: &str,
) -> Result<(), ServerFnErrorErr> {
    use std::collections::hash_map::Entry;

    let session_token_expiration_time = chrono::Duration::days(7);
    match session_tokens.0.entry(session_token.to_string()) {
        Entry::Occupied(occupied) => {
            let minted_time = occupied.get();
            let now = chrono::Utc::now();

            if now.signed_duration_since(minted_time) > session_token_expiration_time {
                occupied.remove();
                Err(ServerFnErrorErr::ServerError(
                    "Invalid session token".to_string(),
                ))
            } else {
                Ok(())
            }
        }
        Entry::Vacant(_vacant) => Err(ServerFnErrorErr::ServerError(
            "Invalid session token".to_string(),
        )),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusMessageType {
    Success,
    Information,
    Error,
}

impl StatusMessageType {
    pub fn to_class(&self) -> &str {
        match self {
            StatusMessageType::Success => "status-message-success",
            StatusMessageType::Information => "status-message-information",
            StatusMessageType::Error => "status-message-error",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GlobalState {
    status_message: Option<(String, StatusMessageType)>,
    visible_command_id: Option<CommandId>,
    command_outputs: HashMap<CommandId, (usize, Vec<u8>)>,
    message_queue: Vec<ClientMessage>,
    session_token: Option<String>,
}

pub mod status {
    use crate::StatusMessageType;

    use super::GlobalState;
    use leptos::*;

    pub fn display_status_message(status_message: String) {
        let state = expect_context::<RwSignal<GlobalState>>();

        state.update(|state| {
            state.status_message = Some((status_message, StatusMessageType::Information))
        });
    }

    pub fn clear_status_message() {
        let state = expect_context::<RwSignal<GlobalState>>();

        state.update(|state| state.status_message = None);
    }

    #[component]
    pub fn StatusDisplay() -> impl IntoView {
        let state = expect_context::<RwSignal<GlobalState>>();

        let status_message = create_memo(move |_| state.get().status_message);

        view! {
            move || {
                match status_message() {
                    Some((status_message, message_type)) => {
                       let class = message_type.to_class().to_owned();
                       view! {
                           <div class=class>{status_message}</div>
                       }
                    },
                    None => {
                       view! {
                           <div class="status-message"></div>
                       }

                    }
                }
            }
        }
    }
}
