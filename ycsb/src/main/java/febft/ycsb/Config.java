package febft.ycsb;

import java.util.Map;
import java.util.HashMap;
import java.io.IOException;
import java.io.BufferedReader;
import java.io.FileReader;

public class Config {
    private static final String REPLICAS_PATH = "config/replicas.config";
    private static Map<Integer, Entry> REPLICAS = null;

    private static final String CLIENTS_PATH = "config/clients.config";
    private static Map<Integer, Entry> CLIENTS = null;

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

            if (entries.length != 4 || entries[0].charAt(0) == '#') {
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
