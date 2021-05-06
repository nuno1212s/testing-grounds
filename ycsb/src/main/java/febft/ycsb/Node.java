package febft.ycsb;

// TLS in Java:
// https://blog.gypsyengineer.com/en/security/an-example-of-tls-13-client-and-server-on-java.html

import java.util.Map;
import java.util.HashMap;
import java.nio.ByteBuffer;

import javax.net.ssl.SSLSocket;

import static febft.ycsb.Config.Entry;
import febft.ycsb.Config;
import febft.ycsb.IdCounter;

public class Node {
    private Entry config;
    private Map<Integer, SSLSocket> tx;
    private Map<Integer, SSLSocket> rx;

    public Node() {
        config = Config.getClients().get(new Integer(IdCounter.nextId()));
    }

    public void bootstrap() throws Exception {
        Map<Integer, Entry> replicas = Config.getReplicas();

        // connect to replicas


        // accept conns from replicas
    }

    public Entry getConfig() {
        return config;
    }
}
