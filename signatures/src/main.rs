use hacl_star::ed25519 as hed;
use sodiumoxide::crypto::sign::ed25519 as sed;

fn main() {
    sodiumoxide::init()
        .expect("Failed to init sodiumoxide!");

    println!("Generating new key pair (using sodiumoxide)...");
    let (_, sodium_sk) = sed::gen_keypair();

    println!("Converting sodiumoxide key to hacl key...");
    let hacl_sk = into_owned_key(&sodium_sk.0[..32])
        .map(hed::SecretKey)
        .expect("Invalid key length?!");
}

fn into_owned_key(s: &[u8]) -> Option<[u8; 32]> {
    if s.len() != 32 {
        return None;
    }
    let mut keybuf = [0; 32];
    keybuf.copy_from_slice(s);
    Some(keybuf)
}
