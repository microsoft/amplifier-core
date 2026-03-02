fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../../proto/amplifier_module.proto";
    let proto_include = "../../proto";

    println!("cargo:rerun-if-changed={proto_file}");
    println!("cargo:rerun-if-changed={proto_include}");

    // Generated code is written to src/generated/ (not OUT_DIR) so it can be
    // committed to the repository. This lets the crate build without requiring
    // protoc to be installed, which simplifies CI and contributor onboarding.
    let out_dir = "src/generated";
    std::fs::create_dir_all(out_dir)?;

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(out_dir)
        .compile_protos(&[proto_file], &[proto_include])?;

    Ok(())
}
