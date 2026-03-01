fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../../proto/amplifier_module.proto";

    println!("cargo:rerun-if-changed={proto_file}");

    let out_dir = "src/generated";
    std::fs::create_dir_all(out_dir)?;

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(out_dir)
        .compile_protos(&[proto_file], &["../../proto"])?;

    Ok(())
}
