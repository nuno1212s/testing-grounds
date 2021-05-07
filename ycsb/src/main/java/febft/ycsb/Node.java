package febft.ycsb;

// TLS in Java:
// https://blog.gypsyengineer.com/en/security/an-example-of-tls-13-client-and-server-on-java.html

import java.util.Random;
import java.util.Map;
import java.util.HashMap;
import java.util.List;
import java.util.Arrays;
import java.nio.ByteBuffer;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import static java.nio.ByteOrder.LITTLE_ENDIAN;

import javax.net.ssl.SSLSocket;
import javax.net.ssl.SSLServerSocket;
import javax.net.ssl.SSLSocketFactory;
import javax.net.ssl.SSLServerSocketFactory;
import javax.net.ssl.SSLParameters;
import javax.net.ssl.SNIServerName;
import javax.net.ssl.SNIHostName;

import static febft.ycsb.Config.Entry;
import febft.ycsb.Config;
import febft.ycsb.IdCounter;

public class Node {
    private static final int BUF_CAP = 1024 * 1024; // 1 MiB

    private static final String[] PROTOCOLS = {"TLSv1.3"};
    private static final String[] CIPHER_SUITES = {"TLS_AES_128_GCM_SHA256"};

    private Entry config;
    private SSLServerSocket listener = null;
    private Map<Integer, OutputStream> tx;
    private Map<Integer, InputStream> rx;
    private ByteBuffer txBuf;
    private Random rng;

    public Node() {
        config = Config.getClients().get(new Integer(IdCounter.nextId()));
        txBuf = ByteBuffer.allocateDirect(BUF_CAP).order(LITTLE_ENDIAN);
        rng = new Random();
    }

    public void close() {
        // NOTE: only close the listener for now,
        // since we haven't implemented handling
        // disconnected clients in `febft` yet
        if (listener != null) {
            try {
                listener.close();
            } catch (IOException e) {
                // noop
            } finally {
                listener = null;
            }
        }
    }

    public void bootstrap() throws IOException {
        listener = listen(config.getHostname(), config.getPortNo());

        int noReplicas = 0;
        final Map<Integer, Entry> replicas = Config.getReplicas();

        this.tx = new HashMap<>();
        this.rx = new HashMap<>();

        // connect to replicas
        for (Entry replicaConfig : replicas.values()) {
            OutputStream writer = connect(
                replicaConfig.getHostname(),
                replicaConfig.getIpAddr(),
                replicaConfig.getPortNo()
            );

            Header header = new Header(
                config.getId(),
                replicaConfig.getId(),
                rng.nextLong(),
                null
            );

            txBuf.clear();
            header.serializeInto(txBuf);
            writer.write(txBuf.array(), 0, txBuf.position());

            tx.put(replicaConfig.getId(), writer);
            noReplicas++;
        }

        // accept conns from replicas
        for (int i = 0; i < noReplicas; i++) {
            SSLSocket socket = (SSLSocket)listener.accept();
            InputStream reader = socket.getInputStream();

            txBuf.clear();
            reader.read(txBuf.array(), 0, Header.LENGTH);
            txBuf.limit(Header.LENGTH);

            Header header = Header.deserializeFrom(txBuf);
            rx.put(header.getFrom(), reader);
        }
    }

    public Entry getConfig() {
        return config;
    }
    
    
    private static OutputStream connect(String sni, String host, int port) throws IOException {
        SSLSocket socket = (SSLSocket)
            SSLSocketFactory.getDefault().createSocket(host, port);

        SSLParameters params = socket.getSSLParameters();
        List<SNIServerName> serverNames = Arrays.asList(new SNIHostName(sni));

        params.setProtocols(PROTOCOLS);
        params.setCipherSuites(CIPHER_SUITES);
        params.setServerNames(serverNames);

        socket.setSSLParameters(params);
        return socket.getOutputStream();
    }
    
    private static SSLServerSocket listen(String sni, int port) throws IOException {
        SSLServerSocket socket = (SSLServerSocket)
            SSLServerSocketFactory.getDefault().createServerSocket(port);

        SSLParameters params = socket.getSSLParameters();
        List<SNIServerName> serverNames = Arrays.asList(new SNIHostName(sni));

        params.setProtocols(PROTOCOLS);
        params.setCipherSuites(CIPHER_SUITES);
        params.setServerNames(serverNames);

        socket.setSSLParameters(params);
        return socket;
    }
}
