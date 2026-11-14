// Skua: delay-load `advapi32` in the `find` executable (MSVC).
//
// Only the permission tests (`-readable`, `-writable`, `-executable`) and the owner and group
// predicates call into it, but the loader mapped it, and the DLLs it needs (sechost, rpcrt4),
// at every start. With `/DELAYLOAD` it is loaded at the first call.

use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
        && env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
    {
        println!("cargo:rustc-link-arg-bin=find=/DELAYLOAD:advapi32.dll");
        println!("cargo:rustc-link-arg-bin=find=delayimp.lib");
    }
}
