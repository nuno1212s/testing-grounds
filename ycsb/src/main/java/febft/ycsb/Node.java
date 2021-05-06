package febft.ycsb;

// TLS in Java:
// https://blog.gypsyengineer.com/en/security/an-example-of-tls-13-client-and-server-on-java.html

import java.nio.ByteBuffer;

import static febft.ycsb.Config.Entry;
import febft.ycsb.Config;
import febft.ycsb.IdCounter;

public class Node {
    private Entry config;

    public Node() {
        config = Config.getClients().get(new Integer(IdCounter.nextId()));
    }

    public void bootstrap() throws Exception {
    }

    public Entry getConfig() {
        return config;
    }
}
