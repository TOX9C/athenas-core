use athena_frontend::App;

fn main() {
    console_error_panic_hook::set_once();
    dioxus::prelude::launch(App);
}
