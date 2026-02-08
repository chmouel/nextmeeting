fn main() {
    println!("cargo:rerun-if-changed=resources");
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/nextmeeting.gresource.xml",
        "nextmeeting.gresource",
    );
}
