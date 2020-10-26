use std::thread;
use std::time::Duration;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use hacl_star::ed25519 as hed;
use sodiumoxide::crypto::sign::ed25519 as sed;

const SECS: u64 = 5;
const TIME: Duration = Duration::from_secs(SECS);
const MSG: &str = "test12345";

macro_rules! test {
    (msg: $msg:expr, op: $op:expr) => {{
        println!($msg);
        let ops = testcase(move |quit| {
            let mut counter = 0;
            while !quit.load(Ordering::Relaxed) {
                let _result = $op;
                counter += 1;
            }
            counter
        }).expect("Failed to run test case!");
        println!("{} per second", ops_per_sec(ops));
    }}
}

fn main() {
    sodiumoxide::init()
        .expect("Failed to init sodiumoxide!");

    println!("Generating new key pair (using sodiumoxide)...");
    let (_, sodium_sk) = sed::gen_keypair();

    println!("Converting sodiumoxide key to hacl key...");
    let hacl_sk = into_owned_key(&sodium_sk.0[..32])
        .map(hed::SecretKey)
        .expect("Invalid key length?!");

    let sk = sodium_sk.clone();
    test! {
        msg: "Testing throughput of sodiumoxide signatures... ",
        op: sed::sign_detached(MSG.as_ref(), &sk)
    };

    let sk = hacl_sk.clone();
    test! {
        msg: "Testing throughput of hacl signatures... ",
        op: sk.signature(MSG.as_ref())
    };
}

fn ops_per_sec(ops: u64) -> f64 {
    (ops as f64) / (SECS as f64)
}

fn testcase<F>(job: F) -> thread::Result<u64>
where
    F: 'static + Send + FnOnce(Arc<AtomicBool>) -> u64
{
    let quit = Arc::new(AtomicBool::new(false));
    let quit_clone = quit.clone();
    let handle = thread::spawn(|| job(quit_clone));
    thread::sleep(TIME);
    quit.store(true, Ordering::Relaxed);
    handle.join()
}

fn into_owned_key(s: &[u8]) -> Option<[u8; 32]> {
    if s.len() != 32 {
        return None;
    }
    let mut keybuf = [0; 32];
    keybuf.copy_from_slice(s);
    Some(keybuf)
}
