package bftsmartest;

import java.util.concurrent.atomic.AtomicInteger;

public class IdCounter {
    private static AtomicInteger counter = new AtomicInteger();

    public static int nextId() {
        return 1000 + counter.getAndAdd(1);
    }
}
