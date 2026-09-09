mod core_bridge;
mod window;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

fn main() -> glib::ExitCode {
    adw::init().expect("libadwaita 初始化失败");
    let app = adw::Application::builder()
        .application_id("app.lithe.linux")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_startup(|app| window::create(app));
    app.connect_open(move |_, files, _| window::open_files(files));
    app.run()
}
