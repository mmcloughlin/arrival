use std::env;

fn write_build_meta() {
    let out_dir = env::var("OUT_DIR").expect("The OUT_DIR environment variable must be set");

    let build_profile = env::var("PROFILE").expect("The PROFILE environment variable must be set");

    std::fs::write(
        std::path::Path::new(&out_dir).join("meta.rs"),
        format!(
            "/// Build profile. \n\
            pub const BUILD_PROFILE: &str = \"{build_profile}\";"
        ),
    )
    .unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=filetests");

    write_build_meta();
}
