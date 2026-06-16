use athena_frontend::App;

fn main() {
    console_error_panic_hook::set_once();

    web_sys::console::log_1(&"[BOOT] main.rs entry reached".into());

    dioxus::prelude::launch(App);

    web_sys::console::log_1(&"[BOOT] dioxus::launch returned".into());
}
