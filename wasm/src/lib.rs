#[link(wasm_import_module = "env")]
extern "C" {
    fn log(x: i32);
}

#[no_mangle]
pub extern "C" fn add(x: i32, y: i32) -> i32 {
    x + y
}

#[no_mangle]
pub unsafe extern "C" fn add_print(x: i32, y: i32) {
    log(x + y);
}

// more info:
// https://stackoverflow.com/questions/49014610/passing-a-javascript-string-to-a-rust-function-compiled-to-webassembly
// https://depth-first.com/articles/2020/06/29/compiling-rust-to-webassembly-a-simple-example/
// https://doc.rust-lang.org/reference/items/external-blocks.html#the-link-attribute
