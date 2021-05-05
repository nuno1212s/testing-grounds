package febft.ycsb;

// TLS in Java:
// https://blog.gypsyengineer.com/en/security/an-example-of-tls-13-client-and-server-on-java.html

impot febft.ycsb.IdCounter;

public class Node {
    private int id;

    public Node() {
        this.id = IdCounter.nextId();
    }
}
