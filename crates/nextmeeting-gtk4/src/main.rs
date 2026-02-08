use nextmeeting_gtk4::application::GtkApp;

fn main() {
    let app = GtkApp::new().expect("failed to initialise GTK application runtime");
    let _ = app.run();
}
