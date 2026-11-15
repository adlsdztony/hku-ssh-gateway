use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=HKU_OPENCONNECT_BUNDLE");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("openconnect-runtime.tar.gz");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        if let Some(source) = env::var_os("HKU_OPENCONNECT_BUNDLE") {
            let source = fs::canonicalize(source).expect("Cannot find OpenConnect runtime bundle");
            println!("cargo:rerun-if-changed={}", source.display());
            assert!(
                fs::metadata(&source).unwrap().len() > 0,
                "Runtime bundle is empty"
            );
            fs::copy(source, output).expect("Cannot embed OpenConnect runtime");
            return;
        }
    }
    fs::write(output, []).unwrap();
}
