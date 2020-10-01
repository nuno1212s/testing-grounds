use bencher::Bencher;

pub fn bench_write(b: &mut Bencher) {
    let mut x = 0;
    b.iter(|| {
        // placeholder
        x += 1;
    });
}
