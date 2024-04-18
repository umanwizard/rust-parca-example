fn main() {
    tonic_build::configure()
        .compile(&["protos/profilestore.proto"], &["protos/googleapis", "protos"])
        .unwrap();
}
