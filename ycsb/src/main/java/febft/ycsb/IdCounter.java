package febft.ycsb;

import java.util.Properties;
import java.util.concurrent.atomic.AtomicInteger;

public class IdCounter {
    private static AtomicInteger counter = new AtomicInteger();

    public static int nextId() {
        Properties props = System.getProperties();
        int initId = Integer.valueOf((String)props.get("smart-initkey"));
        return initId + counter.addAndGet(1);
    }
}
