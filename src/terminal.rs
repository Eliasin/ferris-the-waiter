use wasm_bindgen::prelude::*;
use web_sys::js_sys::Object;

use crate::{CommandId, GlobalState};
use leptos::html::Div;
use leptos::*;

#[wasm_bindgen]
extern "C" {
    pub type Terminal;

    #[wasm_bindgen(constructor)]
    pub fn new() -> Terminal;

    #[wasm_bindgen(constructor)]
    pub fn new_with_options(options: &JsValue) -> Terminal;

    #[wasm_bindgen(method)]
    pub fn open(this: &Terminal, element: &web_sys::HtmlElement);

    #[wasm_bindgen(method)]
    pub fn write(this: &Terminal, data: &[u8]);
}

#[component]
pub fn TerminalFrontend(command_id: CommandId) -> impl IntoView {
    let state = expect_context::<RwSignal<GlobalState>>();

    let visible_command_id = create_memo(move |_| state().visible_command_id);
    let (unwritten_output, set_unwritten_output) = create_slice(
        state,
        move |state| state.command_outputs.get(&command_id).cloned(),
        move |state, new_command_output| {
            state.command_outputs.insert(command_id, new_command_output);
        },
    );

    let div_ref = create_node_ref::<Div>();
    let (terminal, set_terminal) = create_signal::<Option<Terminal>>(None);

    create_effect(move |_| {
        if let Some(div_ref) = div_ref() {
            let needs_init = terminal.with(Option::is_none);
            if needs_init {
                let options = Object::new();
                web_sys::js_sys::Reflect::set(&options, &"convertEol".into(), &true.into())
                    .unwrap();
                web_sys::js_sys::Reflect::set(&options, &"disableStdin".into(), &true.into())
                    .unwrap();

                let terminal = Terminal::new_with_options(&options);
                terminal.open(&div_ref);

                set_terminal.set(Some(terminal));
            }
        }
    });

    create_effect(move |_| {
        let unwritten_output = unwritten_output.get();
        terminal.with(|terminal| {
            if let Some(terminal) = terminal {
                if let Some(unwritten_output) = unwritten_output {
                    terminal.write(&unwritten_output.1);
                    set_unwritten_output((&unwritten_output.0 + 1, vec![]));
                }
            }
        });
    });

    create_effect(move |_| {
        let class = match visible_command_id() {
            Some(visible_command_id) => {
                if command_id == visible_command_id {
                    "terminal"
                } else {
                    "terminal-hidden"
                }
            }
            None => "terminal-hidden",
        };

        if let Some(div_ref) = div_ref() {
            div_ref.set_class_name(class);
        }
    });

    view! {
        <div id=format!("command-output-{}", command_id.0) node_ref=div_ref id="terminal"></div>
    }
}
