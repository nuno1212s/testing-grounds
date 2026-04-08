use std::io::{self, Read, BufReader};
use wasmer_runtime::types::Value;

fn main() {
    // boilerplate to get stdin handle
    let stdin = io::stdin();
    let stdin_lock = stdin.lock();
    let mut stdin_buf = BufReader::new(stdin_lock);

    // wasm environment
    let imports = wasmer_runtime::imports! {
        "env" => {
            "log" => wasmer_runtime::func!(wasm_log),
        },
    };

    // read wasm into buffer from stdin
    let mut wasm = Vec::new();
    stdin_buf.read_to_end(&mut wasm)
        .expect("failed to read wasm");

    // call wasm
    let instance = wasmer_runtime::instantiate(&wasm, &imports)
        .expect("failed to instantiate wasm");
    let result = instance.call("add_print", &[Value::I32(41), Value::I32(1)])
        .expect("failed to call add_print");

    println!("{:?}", result);
}

fn wasm_log(x: i32) {
    println!("{}", x);
}
