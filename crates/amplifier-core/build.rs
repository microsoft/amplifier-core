fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../../proto/amplifier_module.proto";
    let proto_include = "../../proto";

    println!("cargo:rerun-if-changed={proto_file}");
    println!("cargo:rerun-if-changed={proto_include}");

    // Generated code is written to src/generated/ (not OUT_DIR) so it can be
    // committed to the repository. This lets the crate build without requiring
    // protoc to be installed, which simplifies CI and contributor onboarding.
    //
    // If protoc is not found, we skip regeneration and use the pre-committed
    // generated stubs. Regeneration only happens on dev machines with protoc.
    let out_dir = "src/generated";
    std::fs::create_dir_all(out_dir)?;

    let has_protoc = std::process::Command::new("protoc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_protoc {
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .out_dir(out_dir)
            .compile_protos(&[proto_file], &[proto_include])?;
    } else {
        println!(
            "cargo:warning=protoc not found — using pre-committed generated stubs in src/generated/"
        );
    }

    Ok(())
}
