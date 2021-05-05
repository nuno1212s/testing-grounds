package febft.ycsb;

import java.util.Map;
import java.io.IOException;
import java.security.Security;

import org.bouncycastle.jce.provider.BouncyCastleProvider;

import febft.ycsb.Node;

import org.apache.log4j.FileAppender;
import org.apache.log4j.SimpleLayout;
import org.apache.log4j.BasicConfigurator;

import site.ycsb.ByteIterator;
import site.ycsb.Status;
import site.ycsb.DB;

public class YCSBClient extends DB {
    private Node node;

    public YCSBClient() {
        // empty constructor
    }

    public void init() {
        try {
            String sep = System.getProperty("file.separator");
            BasicConfigurator.configure(new FileAppender(new SimpleLayout(), String.format("log%s%d", sep, Node.ID), false));
        } catch (IOException e) {
            System.err.println("Failed to create log file.");
            System.exit(1);
        }

        Security.addProvider(new BouncyCastleProvider());

        System.setProperty("javax.net.ssl.keyStore", "ca-root/keystore.jks");
        System.setProperty("javax.net.ssl.keyStorePassword", "123456");

        System.setProperty("javax.net.ssl.trustStore", "ca-root/truststore.jks");
        System.setProperty("javax.net.ssl.trustStorePassword", "123456");
    }

    @Override
    public Status update(String table, String key, Map<String, ByteIterator> values) {
        // TODO: implement update
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status read(String table, String key, Set<String> fields, Map<String, ByteIterator> result) {
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status scan(String table, String startkey, int recordcount, Set<String> fields,
                       Vector<HashMap<String, ByteIterator>> result) {
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status insert(String table, String key, Map<String, ByteIterator> values) {
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status delete(String table, String key) {
        return Status.NOT_IMPLEMENTED;
    }
}
