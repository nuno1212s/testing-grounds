use std::thread;
use std::time::Duration;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use sodiumoxide::crypto::auth::hmacsha256 as smac;

//use hacl_star::ed25519 as hed;
use hacl_star_gcc::ed25519 as ged;
use sodiumoxide::crypto::sign::ed25519 as sed;
use ring::{signature as red, signature::KeyPair};

const SECS: u64 = 5;
const TIME: Duration = Duration::from_secs(SECS);

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
        })
            .map(ops_per_sec)
            .expect("Failed to run test case!");
        println!("  --> {} ops per second", ops);
        eprint!("{},", ops);
    }}
}

fn main() {
    sodiumoxide::init()
        .expect("Failed to init sodiumoxide!");

    let msg_size = std::env::args()
        .nth(1)
        .and_then(|x| x.parse().ok())
        .unwrap_or(1024);
    let msg_original = vec![0; msg_size];

    println!("* Generating HMAC key (using sodiumoxide)...");
    let sodium_hmac_key = smac::gen_key();

    println!("* Generating new key pair (using sodiumoxide)...");
    let (sodium_pk, sodium_sk) = sed::gen_keypair();

    println!("* Generating new key pair (using openssl)...");
    let openssl_sk = openssl::pkey::PKey::<openssl::pkey::Private>::generate_ed25519().unwrap();

    println!("* Converting sodiumoxide keys to hacl keys...");
    //let (hacl_pk, hacl_sk) = {
    //    let sk = hed::SecretKey(into_owned_key(&sodium_sk));
    //    let pk = sk.get_public();
    //    (pk, sk)
    //};
    let (hacl_gcc_pk, hacl_gcc_sk) = {
        let sk = ged::SecretKey(into_owned_key(&sodium_sk));
        let pk = sk.get_public();
        (pk, sk)
    };

    eprintln!("sign-hmac-sodiumoxide,sign-ed25519-sodiumoxide,sign-ed25519-hacl-gcc,sign-ed25519-ring,sign-ed25519-openssl,verify-hmac-sodiumoxide,verify-ed25519-sodiumoxide,verify-ed25519-hacl-gcc,verify-ed25519-ring,verify-ed25519-openssl");

    let sk = sodium_hmac_key.clone();
    let msg_cloned = msg_original.clone();
    test! {
        msg: "* Testing throughput of sodiumoxide hmac signatures...",
        op: smac::authenticate(msg_cloned.as_ref(), &sk)
    };

    let sk = sodium_sk.clone();
    let msg_cloned = msg_original.clone();
    test! {
        msg: "* Testing throughput of sodiumoxide signatures...",
        op: sed::sign_detached(msg_cloned.as_ref(), &sk)
    };

    //let sk = hacl_sk.clone();
    //test! {
    //    msg: "* Testing throughput of hacl signatures...",
    //    op: sk.signature(msg_cloned.as_ref())
    //};

    let sk = hacl_gcc_sk.clone();
    let msg_cloned = msg_original.clone();
    test! {
        msg: "* Testing throughput of hacl-gcc signatures...",
        op: sk.signature(msg_cloned.as_ref())
    };

    let sk = red::Ed25519KeyPair::from_seed_unchecked(&sodium_sk.0[..32]).unwrap();
    let msg_cloned = msg_original.clone();
    test! {
        msg: "* Testing throughput of ring signatures...",
        op: sk.sign(msg_cloned.as_ref())
    };

    let sk = openssl_sk.clone();
    let msg_cloned = msg_original.clone();
    test! {
        msg: "* Testing throughput of openssl signatures...",
        op: openssl_sign(&sk, msg_cloned.as_ref())
    };

    let pk = sodium_hmac_key.clone();
    let msg_cloned = msg_original.clone();
    let sig = smac::authenticate(msg_cloned.as_ref(), &sodium_hmac_key);
    test! {
        msg: "* Testing throughput of sodiumoxide hmac verifying...",
        op: assert!(smac::verify(&sig, msg_cloned.as_ref(), &pk))
    };

    let pk = sodium_pk.clone();
    let msg_cloned = msg_original.clone();
    let sig = sed::sign_detached(msg_cloned.as_ref(), &sodium_sk);
    test! {
        msg: "* Testing throughput of sodiumoxide verifying...",
        op: assert!(sed::verify_detached(&sig, msg_cloned.as_ref(), &pk))
    };

    //let pk = hacl_pk.clone();
    //let sig = hacl_sk.signature(msg_cloned.as_ref());
    //test! {
    //    msg: "* Testing throughput of hacl verifying...",
    //    op: {
    //        let pk = pk.clone();
    //        assert!(pk.verify(msg_cloned.as_ref(), &sig))
    //    }
    //};

    let pk = hacl_gcc_pk.clone();
    let msg_cloned = msg_original.clone();
    let sig = hacl_gcc_sk.signature(msg_cloned.as_ref());
    test! {
        msg: "* Testing throughput of hacl-gcc verifying...",
        op: assert!(pk.verify(msg_cloned.as_ref(), &sig))
    };

    let msg_cloned = msg_original.clone();
    let sk = red::Ed25519KeyPair::from_seed_unchecked(&sodium_sk.0[..32]).unwrap();
    let pk = sk.public_key().clone();
    let pk = red::UnparsedPublicKey::new(&red::ED25519, pk);
    let sig = sk.sign(msg_cloned.as_ref());
    test! {
        msg: "* Testing throughput of ring verifying...",
        op: assert!(pk.verify(msg_cloned.as_ref(), sig.as_ref()).is_ok())
    };

    let pk = openssl_sk.clone();
    let msg_cloned = msg_original.clone();
    let sig = openssl_sign(&pk, msg_cloned.as_ref());
    test! {
        msg: "* Testing throughput of openssl verifying...",
        op: assert!(openssl_verify(&pk, msg_cloned.as_ref(), &sig))
    };

    eprintln!("\x08 ");
}

fn openssl_sign(sk: &openssl::pkey::PKey<openssl::pkey::Private>, data: &[u8]) -> [u8; 64] {
    let mut signature = [0; 64];
    let mut signer = openssl::sign::Signer::new_without_digest(sk).unwrap();
    signer.sign_oneshot(&mut signature[..], data).unwrap();
    signature
}

fn openssl_verify(sk: &openssl::pkey::PKey<openssl::pkey::Private>, data: &[u8], sig: &[u8; 64]) -> bool {
    let mut verifier = openssl::sign::Verifier::new_without_digest(sk).unwrap();
    verifier.verify_oneshot(&sig[..], data).unwrap()
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

fn into_owned_key(sed::SecretKey(ref s): &sed::SecretKey) -> [u8; 32] {
    let mut skbuf = [0; 32];
    skbuf.copy_from_slice(&s[..32]);
    skbuf
}
