package febft.ycsb;

import java.util.Map;
import java.util.HashMap;
import java.util.Scanner;
import java.io.IOException;
import java.io.BufferedReader;
import java.io.FileInputStream;
import java.io.FileReader;
import java.io.File;

import javax.net.ssl.SSLSocketFactory;
import javax.net.ssl.SSLServerSocketFactory;

import nl.altindag.ssl.SSLFactory;

public class Config {
    private static final String CLI_PREFIX = "cli";
    private static final int CLI_BASE = 1000;
    private static final int CLI_AMT = 1000;
    private static final int CLI_COUNT = CLI_BASE + CLI_AMT;

    private static final String CA_ROOT_PATH = "ca-root";
    private static SSLFactory SSL_FAC = null;
    private static Object SSL_FAC_MUX = new Object();

    private static final String REPLICAS_PATH = "config/replicas.config";
    private static Map<Integer, Entry> REPLICAS = null;

    private static final String CLIENTS_PATH = "config/clients.config";
    private static Map<Integer, Entry> CLIENTS = null;

    private static final String BATCH_SIZE_PATH = "config/batch.config";
    private static int BATCH_SIZE = 0;

    public static SSLSocketFactory getSslSocketFactory() {
        synchronized (SSL_FAC_MUX) {
            return getSslFactory().getSslSocketFactory();
        }
    }

    public static SSLServerSocketFactory getSslServerSocketFactory() {
        synchronized (SSL_FAC_MUX) {
            return getSslFactory().getSslServerSocketFactory();
        }
    }

    private static SSLFactory getSslFactory() {
        if (SSL_FAC == null) {
            final char[] password = "123456".toCharArray();
            SSLFactory.Builder sslFactoryBuilder = null;

            try (FileInputStream file = new FileInputStream(CA_ROOT_PATH + "/truststore.jks")) {
                sslFactoryBuilder = SSLFactory.builder()
                    .withTrustMaterial(file, password, "jks");

                for (int i = CLI_BASE; i < CLI_COUNT; ++i) {
                    final String path = String.format("%s/%s%d.pfx", CA_ROOT_PATH, CLI_PREFIX, i);

                    try (FileInputStream file2 = new FileInputStream(path)) {
                        sslFactoryBuilder = sslFactoryBuilder
                            .withIdentityMaterial(file2, password);
                    } catch (SecurityException | IOException e) {
                        throw new RuntimeException(e);
                    }
                }
            } catch (SecurityException | IOException e) {
                throw new RuntimeException(e);
            }

            SSL_FAC = sslFactoryBuilder.build();
        }
        return SSL_FAC;
    }

    public synchronized static int getBatchSize() {
        if (BATCH_SIZE != 0) {
            return BATCH_SIZE;
        }
        BATCH_SIZE = parseBatch(BATCH_SIZE_PATH);
        return BATCH_SIZE;
    }

    public synchronized static Map<Integer, Entry> getClients() {
        if (CLIENTS != null) {
            return CLIENTS;
        }
        CLIENTS = parse(CLIENTS_PATH);
        return CLIENTS; 
    }

    public synchronized static Map<Integer, Entry> getReplicas() {
        if (REPLICAS != null) {
            return REPLICAS;
        }
        REPLICAS = parse(REPLICAS_PATH);
        return REPLICAS; 
    }

    private static int parseBatch(String path) {
        int batchSize = -1;
        try (Scanner scanner = new Scanner(new File(BATCH_SIZE_PATH))) {
            batchSize = scanner.nextInt();
        } catch (Exception e) {
            // noop
        } finally {
            return batchSize;
        }
    }

    private static Map<Integer, Entry> parse(String path) {
        String configLine;
        Map<Integer, Entry> config = new HashMap<>();

        try (BufferedReader reader = new BufferedReader(new FileReader(path))) {
            while ((configLine = reader.readLine()) != null) {
                Entry entry = Entry.parse(configLine);
                if (entry != null) {
                    config.put(entry.getId(), entry);
                }
            }
        } catch (IOException e) {
            // noop
        } finally {
            return config;
        }
    }

    public static class Entry {
        private int id, portNo;
        private String hostname, ipAddr;

        public static Entry parse(String configLine) {
            final String[] entries = configLine.trim().split("([ ]+)", 4);

            if (entries.length == 4 && entries[0].charAt(0) != '#') {
                // noop
            } else {
                return null;
            }

            final int id = Integer.parseInt(entries[0]);
            final String hostname = entries[1];
            final String ipAddr = entries[2];
            final int portNo = Integer.parseInt(entries[3]);

            return new Entry(id, hostname, ipAddr, portNo);
        }

        public Entry(int id, String hostname, String ipAddr, int portNo) {
            this.id = id;
            this.hostname = hostname;
            this.ipAddr = ipAddr;
            this.portNo = portNo;
        }

        public int getId() {
            return id;
        }

        public String getHostname() {
            return hostname;
        }

        public String getIpAddr() {
            return ipAddr;
        }

        public int getPortNo() {
            return portNo;
        }
    }
}
