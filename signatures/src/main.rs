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

fn main() {
    sodiumoxide::init()
        .expect("Failed to init sodiumoxide!");

    println!("Generating new key pair (using sodiumoxide)...");
    let (_, sodium_sk) = sed::gen_keypair();

    println!("Converting sodiumoxide key to hacl key...");
    let hacl_sk = into_owned_key(&sodium_sk.0[..32])
        .map(hed::SecretKey)
        .expect("Invalid key length?!");

    print!("Testing throughput of sodiumoxide signatures... ");
    let sk = sodium_sk.clone();
    let sigs = testcase(move |quit| {
        let mut counter = 0;
        while !quit.load(Ordering::Relaxed) {
            let _signature = sed::sign_detached(MSG.as_ref(), &sk);
            counter += 1;
        }
        counter
    })
    .expect("Failed to run test case!");
    println!("{} per second", sigs_per_sec(sigs));

    print!("Testing throughput of hacl signatures... ");
    let sk = hacl_sk.clone();
    let sigs = testcase(move |quit| {
        let mut counter = 0;
        while !quit.load(Ordering::Relaxed) {
            let _signature = sk.signature(MSG.as_ref());
            counter += 1;
        }
        counter
    })
    .expect("Failed to run test case!");
    println!("{} per second", sigs_per_sec(sigs));
}

fn sigs_per_sec(sigs: u64) -> f64 {
    (sigs as f64) / (SECS as f64)
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
